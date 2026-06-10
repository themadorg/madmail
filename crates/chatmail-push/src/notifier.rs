// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chatmail_db::DbPool;
use reqwest::Client;
use tokio::sync::Mutex;

use crate::mode::{record_delivery_failure, record_delivery_success};
use crate::stats::record_successful_delivery;
use crate::store::{list_device_tokens, remove_device_token};
use crate::DEFAULT_NOTIFY_URL;

/// Per-request timeout; deliveries slower than this count as failures.
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(20);
const BASE_DELAY_SECS: f64 = 8.0;
const DROP_DEADLINE: Duration = Duration::from_secs(5 * 60 * 60);

struct Inner {
    queue_dir: PathBuf,
    url: String,
    client: Client,
    pool: DbPool,
}

/// Queues device-token POSTs with exponential backoff (persistent crash recovery).
#[derive(Clone)]
pub struct PushNotifier {
    inner: Arc<Inner>,
    /// Serialize startup requeue so it runs once.
    requeue_lock: Arc<Mutex<()>>,
}

impl PushNotifier {
    pub fn new(pool: DbPool, queue_dir: PathBuf, url: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(CONNECTION_TIMEOUT)
            .build()
            .expect("reqwest client");
        Self {
            inner: Arc::new(Inner {
                queue_dir,
                url: url.unwrap_or_else(|| DEFAULT_NOTIFY_URL.to_string()),
                client,
                pool,
            }),
            requeue_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Scan `queue_dir` for pending jobs after restart.
    pub async fn requeue_persistent(&self) {
        let _guard = self.requeue_lock.lock().await;
        let dir = self.inner.queue_dir.clone();
        let n = self.clone();
        tokio::spawn(async move {
            let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
                return;
            };
            while let Ok(Some(ent)) = entries.next_entry().await {
                let path = ent.path();
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if name.ends_with(".tmp") {
                    let _ = tokio::fs::remove_file(&path).await;
                    continue;
                }
                let Ok(text) = tokio::fs::read_to_string(&path).await else {
                    let _ = tokio::fs::remove_file(&path).await;
                    continue;
                };
                let mut lines = text.lines();
                let Some(username) = lines.next().map(|s| s.to_string()) else {
                    let _ = tokio::fs::remove_file(&path).await;
                    continue;
                };
                let Some(_start_ts_s) = lines.next() else {
                    let _ = tokio::fs::remove_file(&path).await;
                    continue;
                };
                let Some(token) = lines.next().map(|s| s.to_string()) else {
                    let _ = tokio::fs::remove_file(&path).await;
                    continue;
                };
                let job = PersistentJob {
                    path,
                    username,
                    token,
                };
                n.spawn_notify_chain(job, 0);
            }
        });
    }

    /// Notify all registered devices for `username` after inbound mail (not self-sent).
    pub fn notify_inbound(&self, username: &str) {
        let n = self.clone();
        let user = username.to_string();
        let pool = self.inner.pool.clone();
        tokio::spawn(async move {
            let Ok(tokens) = list_device_tokens(&pool, &user).await else {
                return;
            };
            if tokens.is_empty() {
                return;
            }
            n.new_message_for_user(&user, &tokens).await;
        });
    }

    async fn new_message_for_user(&self, username: &str, tokens: &[String]) {
        let start_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        for token in tokens {
            let tmp = self
                .inner
                .queue_dir
                .join(format!("{}.tmp", uuid::Uuid::new_v4()));
            let final_path = self.inner.queue_dir.join(uuid::Uuid::new_v4().to_string());
            let body = format!("{username}\n{start_ts}\n{token}");
            if tokio::fs::write(&tmp, body.as_bytes()).await.is_err() {
                continue;
            }
            if tokio::fs::rename(&tmp, &final_path).await.is_err() {
                let _ = tokio::fs::remove_file(&tmp).await;
                continue;
            }
            let job = PersistentJob {
                path: final_path,
                username: username.to_string(),
                token: token.clone(),
            };
            self.spawn_notify_chain(job, 0);
        }
    }

    fn spawn_notify_chain(&self, job: PersistentJob, retry: u32) {
        let n = self.clone();
        tokio::spawn(async move { n.notify_loop(job, retry).await });
    }

    async fn notify_loop(self, job: PersistentJob, mut retry: u32) {
        let start = Instant::now();
        loop {
            if start.elapsed() > DROP_DEADLINE {
                let _ = tokio::fs::remove_file(&job.path).await;
                tracing::error!(user = %job.username, "push notification exceeded deadline");
                record_delivery_failure(&self.inner.pool).await;
                return;
            }

            let delay = if retry == 0 {
                Duration::ZERO
            } else {
                Duration::from_secs_f64(BASE_DELAY_SECS.powi(retry as i32))
            };
            if delay > Duration::ZERO {
                tokio::time::sleep(delay).await;
            }

            let response = self
                .inner
                .client
                .post(&self.inner.url)
                .body(job.token.clone())
                .send()
                .await;

            match response {
                Ok(res) => {
                    let code = res.status();
                    if code.is_success() {
                        let _ = tokio::fs::remove_file(&job.path).await;
                        record_delivery_success();
                        record_successful_delivery();
                        tracing::debug!(user = %job.username, "push notification delivered");
                        return;
                    }
                    if code == reqwest::StatusCode::GONE {
                        let _ =
                            remove_device_token(&self.inner.pool, &job.username, &job.token).await;
                        let _ = tokio::fs::remove_file(&job.path).await;
                        tracing::info!(user = %job.username, "removed stale push token (410 Gone)");
                        record_delivery_failure(&self.inner.pool).await;
                        return;
                    }
                    tracing::warn!(user = %job.username, status = %code, "push notification HTTP error");
                }
                Err(e) => {
                    tracing::warn!(user = %job.username, error = %e, "push notification request failed")
                }
            }

            retry = retry.saturating_add(1);
            if retry > 24 {
                let _ = tokio::fs::remove_file(&job.path).await;
                tracing::error!(user = %job.username, "push notification gave up after retries");
                record_delivery_failure(&self.inner.pool).await;
                return;
            }
        }
    }
}

struct PersistentJob {
    path: PathBuf,
    username: String,
    token: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::push_stats_snapshot;
    use crate::store::upsert_device_token;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn successful_delivery_increments_push_stats() {
        let _guard = crate::stats::test_lock().await;
        crate::stats::reset_for_test();

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/notify"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let pool = chatmail_db::init_memory_db().await.unwrap();
        upsert_device_token(&pool, "alice@test", "device-token-1")
            .await
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let notify_url = format!("{}/notify", server.uri());
        let notifier = PushNotifier::new(pool, dir.path().to_path_buf(), Some(notify_url));

        let before = push_stats_snapshot();
        notifier.notify_inbound("alice@test");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert_eq!(push_stats_snapshot(), before + 1);
    }
}
