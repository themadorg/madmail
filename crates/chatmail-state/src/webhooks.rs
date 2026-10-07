// Copyright (C) 2026 themadorg
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Operator-only metadata notifications. The bounded queue never awaits on mail paths.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chatmail_db::{get_setting, set_setting, DbPool};
use chatmail_types::{ChatmailError, Result};
use hmac::{Hmac, Mac};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use tokio::sync::mpsc;

const SETTINGS_KEY: &str = "__OPERATOR_WEBHOOKS__";
const QUEUE_CAPACITY: usize = 256;
const DEDUP_CAPACITY: usize = 4096;
const QUOTA_DEDUP: Duration = Duration::from_secs(3600);

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    enabled: bool,
    url: String,
    secret: String,
    event_user_registered: bool,
    event_quota_exceeded: bool,
    timeout_seconds: u64,
    retry_attempts: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            secret: String::new(),
            event_user_registered: true,
            event_quota_exceeded: true,
            timeout_seconds: 5,
            retry_attempts: 2,
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebhookPatch {
    enabled: Option<bool>,
    url: Option<String>,
    /// Omitted preserves the secret; an empty string clears it. Never returned.
    secret: Option<String>,
    event_user_registered: Option<bool>,
    event_quota_exceeded: Option<bool>,
    timeout_seconds: Option<u64>,
    retry_attempts: Option<u32>,
}

impl Settings {
    fn apply(&mut self, patch: WebhookPatch) {
        macro_rules! apply {
            ($($field:ident),*) => { $(if let Some(value) = patch.$field { self.$field = value; })* };
        }
        apply!(
            enabled,
            url,
            secret,
            event_user_registered,
            event_quota_exceeded,
            timeout_seconds,
            retry_attempts
        );
    }

    fn validate(&self) -> Result<()> {
        if !(1..=30).contains(&self.timeout_seconds) || self.retry_attempts > 5 {
            return Err(ChatmailError::config(
                "timeout_seconds must be 1..30 and retry_attempts 0..5",
            ));
        }
        if self.secret.len() > 4096 || self.secret.chars().any(char::is_control) {
            return Err(ChatmailError::config("invalid webhook secret"));
        }
        if self.url.is_empty() && !self.enabled {
            return Ok(());
        }
        if self.url.len() > 2048 || self.url.chars().any(char::is_control) {
            return Err(ChatmailError::config("invalid webhook URL"));
        }
        let url =
            Url::parse(&self.url).map_err(|_| ChatmailError::config("invalid webhook URL"))?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if url.host_str().is_none()
            || (url.scheme() != "https" && !(url.scheme() == "http" && local))
        {
            return Err(ChatmailError::config(
                "webhook URL must use https:// (http:// only for loopback testing)",
            ));
        }
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(ChatmailError::config(
                "webhook URL must not contain credentials or a fragment",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RegistrationSource {
    Jit,
    Web,
    Admin,
}

#[derive(Serialize)]
#[serde(tag = "event")]
enum Event {
    #[serde(rename = "user.registered")]
    Registered {
        timestamp: u64,
        username: String,
        source: RegistrationSource,
        registration_token_used: bool,
    },
    #[serde(rename = "user.quota_exceeded")]
    QuotaExceeded {
        timestamp: u64,
        username: String,
        used_bytes: u64,
        max_bytes: u64,
        incoming_bytes: u64,
        path: &'static str,
    },
    #[serde(rename = "webhook.test")]
    Test {
        timestamp: u64,
        message: &'static str,
    },
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn signature(secret: &str, body: &[u8]) -> String {
    // HMAC accepts keys of any size, including a short operator-selected secret.
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

struct ConfigState {
    settings: Settings,
    generation: u64,
}

struct Job {
    body: Vec<u8>,
    generation: u64,
}

struct Inner {
    config: RwLock<ConfigState>,
    pool: DbPool,
    client: Client,
    sender: mpsc::Sender<Job>,
    receiver: Mutex<Option<mpsc::Receiver<Job>>>,
    update_lock: tokio::sync::Mutex<()>,
    quota_dedup: Mutex<HashMap<String, Instant>>,
    successes: AtomicU64,
    failures: AtomicU64,
    dropped: AtomicU64,
}

#[derive(Clone)]
pub struct OperatorWebhooks {
    inner: Arc<Inner>,
}

impl OperatorWebhooks {
    pub fn new(pool: DbPool) -> Self {
        let (sender, receiver) = mpsc::channel(QUEUE_CAPACITY);
        Self {
            inner: Arc::new(Inner {
                config: RwLock::new(ConfigState {
                    settings: Settings::default(),
                    generation: 0,
                }),
                pool,
                client: Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .no_proxy()
                    .connect_timeout(Duration::from_secs(5))
                    .build()
                    .expect("webhook HTTP client"),
                sender,
                receiver: Mutex::new(Some(receiver)),
                update_lock: tokio::sync::Mutex::new(()),
                quota_dedup: Mutex::new(HashMap::new()),
                successes: AtomicU64::new(0),
                failures: AtomicU64::new(0),
                dropped: AtomicU64::new(0),
            }),
        }
    }

    fn start(&self) {
        let Some(mut receiver) = self.inner.receiver.lock().expect("webhook receiver").take()
        else {
            return;
        };
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            while let Some(job) = receiver.recv().await {
                let Some(inner) = weak.upgrade() else {
                    break;
                };
                let webhook = Self { inner };
                let _ = webhook.deliver(&job).await;
            }
        });
    }

    pub async fn hydrate(&self) -> Result<()> {
        let _guard = self.inner.update_lock.lock().await;
        let settings = match get_setting(&self.inner.pool, SETTINGS_KEY).await? {
            Some(value) => serde_json::from_str::<Settings>(&value)
                .map_err(|_| ChatmailError::config("invalid stored webhook settings"))?,
            None => Settings::default(),
        };
        settings.validate()?;
        self.replace(settings);
        self.start();
        Ok(())
    }

    fn replace(&self, settings: Settings) {
        let mut state = self.inner.config.write().expect("webhook config");
        if state.settings != settings {
            state.settings = settings;
            state.generation = state.generation.wrapping_add(1);
        }
    }

    pub async fn update(&self, patch: WebhookPatch) -> Result<()> {
        let _guard = self.inner.update_lock.lock().await;
        let mut settings = self
            .inner
            .config
            .read()
            .expect("webhook config")
            .settings
            .clone();
        settings.apply(patch);
        settings.validate()?;
        let encoded = serde_json::to_string(&settings)
            .map_err(|_| ChatmailError::config("cannot encode webhook settings"))?;
        set_setting(&self.inner.pool, SETTINGS_KEY, &encoded).await?;
        self.replace(settings);
        self.start();
        Ok(())
    }

    /// Redacted operator snapshot. Do not derive Debug/serialize the runtime state.
    pub fn snapshot(&self) -> Value {
        let state = self.inner.config.read().expect("webhook config");
        let s = &state.settings;
        json!({
            "enabled": s.enabled, "url": s.url, "secret_configured": !s.secret.is_empty(),
            "event_user_registered": s.event_user_registered,
            "event_quota_exceeded": s.event_quota_exceeded,
            "timeout_seconds": s.timeout_seconds, "retry_attempts": s.retry_attempts,
            "successful_deliveries": self.inner.successes.load(Ordering::Relaxed),
            "consecutive_failures": self.inner.failures.load(Ordering::Relaxed),
            "dropped_events": self.inner.dropped.load(Ordering::Relaxed),
        })
    }

    fn enqueue(&self, event: &Event) -> bool {
        let state = self.inner.config.read().expect("webhook config");
        let s = &state.settings;
        let allowed = s.enabled
            && match event {
                Event::Registered { .. } => s.event_user_registered,
                Event::QuotaExceeded { .. } => s.event_quota_exceeded,
                Event::Test { .. } => true,
            };
        if !allowed {
            return false;
        }
        let Ok(body) = serde_json::to_vec(event) else {
            return false;
        };
        if self
            .inner
            .sender
            .try_send(Job {
                body,
                generation: state.generation,
            })
            .is_err()
        {
            self.inner.dropped.fetch_add(1, Ordering::Relaxed);
            tracing::warn!("operator webhook queue full; metadata event dropped");
            return false;
        }
        true
    }

    pub fn registered(
        &self,
        username: &str,
        source: RegistrationSource,
        registration_token_used: bool,
    ) {
        self.enqueue(&Event::Registered {
            timestamp: timestamp(),
            username: username.to_owned(),
            source,
            registration_token_used,
        });
    }

    pub fn quota_exceeded(
        &self,
        username: &str,
        used_bytes: u64,
        max_bytes: u64,
        incoming_bytes: u64,
        path: &'static str,
    ) {
        {
            let state = self.inner.config.read().expect("webhook config");
            if !state.settings.enabled || !state.settings.event_quota_exceeded {
                return;
            }
        }
        let mut dedup = self.inner.quota_dedup.lock().expect("webhook quota dedup");
        let now = Instant::now();
        dedup.retain(|_, last| now.duration_since(*last) < QUOTA_DEDUP);
        if dedup.contains_key(username) {
            return;
        }
        if dedup.len() >= DEDUP_CAPACITY {
            // Do not evict an unexpired entry and send duplicate quota alerts.
            self.inner.dropped.fetch_add(1, Ordering::Relaxed);
            tracing::warn!("operator webhook dedup capacity reached; metadata event dropped");
            return;
        }
        if self.enqueue(&Event::QuotaExceeded {
            timestamp: timestamp(),
            username: username.to_owned(),
            used_bytes,
            max_bytes,
            incoming_bytes,
            path,
        }) {
            dedup.insert(username.to_owned(), now);
        }
    }

    pub async fn send_test(&self) -> Result<()> {
        let generation = {
            let state = self.inner.config.read().expect("webhook config");
            if !state.settings.enabled {
                return Err(ChatmailError::config("operator webhooks are disabled"));
            }
            state.generation
        };
        let body = serde_json::to_vec(&Event::Test {
            timestamp: timestamp(),
            message: "madmail operator webhook test",
        })
        .map_err(|_| ChatmailError::config("cannot encode test webhook"))?;
        self.deliver(&Job { body, generation }).await
    }

    async fn deliver(&self, job: &Job) -> Result<()> {
        let settings = {
            let state = self.inner.config.read().expect("webhook config");
            if !state.settings.enabled || state.generation != job.generation {
                return Err(ChatmailError::config(
                    "webhook configuration changed; pending event discarded",
                ));
            }
            state.settings.clone()
        };
        for attempt in 0..=settings.retry_attempts {
            {
                let state = self.inner.config.read().expect("webhook config");
                if !state.settings.enabled || state.generation != job.generation {
                    return Err(ChatmailError::config(
                        "webhook configuration changed; pending event discarded",
                    ));
                }
            }
            let mut request = self
                .inner
                .client
                .post(&settings.url)
                .header("Content-Type", "application/json")
                .timeout(Duration::from_secs(settings.timeout_seconds))
                .body(job.body.clone());
            if !settings.secret.is_empty() {
                request = request.header(
                    "X-Madmail-Signature",
                    signature(&settings.secret, &job.body),
                );
            }
            let result = request.send().await;
            if result
                .as_ref()
                .is_ok_and(|response| response.status().is_success())
            {
                self.inner.successes.fetch_add(1, Ordering::Relaxed);
                self.inner.failures.store(0, Ordering::Relaxed);
                return Ok(());
            }
            self.inner.failures.fetch_add(1, Ordering::Relaxed);
            // Never log reqwest errors: URLs can contain credentials in query strings.
            tracing::warn!(
                attempt,
                status = result.as_ref().ok().map(|r| r.status().as_u16()),
                "operator webhook delivery failed"
            );
            let retryable = result.as_ref().map_or(true, |r| {
                r.status().is_server_error() || r.status().as_u16() == 429
            });
            if !retryable || attempt == settings.retry_attempts {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
        }
        Err(ChatmailError::config("operator webhook delivery failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn configured(server: &MockServer) -> OperatorWebhooks {
        let webhook = OperatorWebhooks::new(chatmail_db::init_memory_db().await.unwrap());
        webhook
            .update(
                serde_json::from_value(json!({
                    "enabled": true, "url": format!("{}/hook", server.uri()),
                    "secret": "test-only-secret", "retry_attempts": 0
                }))
                .unwrap(),
            )
            .await
            .unwrap();
        webhook
    }

    #[test]
    fn hmac_matches_known_sha256_vector() {
        assert_eq!(
            signature("Jefe", b"what do ya want for nothing?"),
            "sha256=5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn url_and_policy_validation() {
        for url in [
            "http://example.com/hook",
            "file:///etc/passwd",
            "https://user:secret@example.com",
            "https://example.com/#fragment",
        ] {
            let s = Settings {
                url: url.into(),
                enabled: true,
                ..Settings::default()
            };
            assert!(s.validate().is_err(), "{url}");
        }
        for url in [
            "https://example.com/hook",
            "http://127.0.0.1:8080/hook",
            "http://[::1]/hook",
            "http://localhost/hook",
        ] {
            assert!(Settings {
                url: url.into(),
                enabled: true,
                ..Settings::default()
            }
            .validate()
            .is_ok());
        }
        assert!(Settings {
            timeout_seconds: 0,
            ..Settings::default()
        }
        .validate()
        .is_err());
        assert!(Settings {
            retry_attempts: 6,
            ..Settings::default()
        }
        .validate()
        .is_err());
        assert!(Settings {
            secret: "bad\nsecret".into(),
            ..Settings::default()
        }
        .validate()
        .is_err());
    }

    #[tokio::test]
    async fn signed_test_payload_stats_and_secret_preservation() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/hook"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let webhook = configured(&server).await;
        webhook
            .update(serde_json::from_value(json!({"timeout_seconds": 2})).unwrap())
            .await
            .unwrap();
        assert!(webhook.snapshot()["secret_configured"].as_bool().unwrap());
        assert!(!webhook.snapshot().to_string().contains("test-only-secret"));
        webhook.send_test().await.unwrap();
        let requests = server.received_requests().await.unwrap();
        let request = &requests[0];
        assert_eq!(
            request.headers["X-Madmail-Signature"].to_str().unwrap(),
            signature("test-only-secret", &request.body)
        );
        assert_eq!(request.headers["Content-Type"], "application/json");
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["event"], "webhook.test");
        assert_eq!(body["message"], "madmail operator webhook test");
        assert!(body["timestamp"].as_u64().unwrap() > 0);
        assert_eq!(webhook.snapshot()["successful_deliveries"], 1);
        webhook
            .update(serde_json::from_value(json!({"secret": ""})).unwrap())
            .await
            .unwrap();
        assert_eq!(webhook.snapshot()["secret_configured"], false);
    }

    #[tokio::test]
    async fn failure_retry_recovery_and_no_redirects() {
        let server = MockServer::start().await;
        let webhook = configured(&server).await;
        webhook
            .update(serde_json::from_value(json!({"retry_attempts": 1})).unwrap())
            .await
            .unwrap();
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .expect(2)
            .mount(&server)
            .await;
        assert!(webhook.send_test().await.is_err());
        assert_eq!(webhook.snapshot()["consecutive_failures"], 2);
        server.reset().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        webhook.send_test().await.unwrap();
        assert_eq!(webhook.snapshot()["consecutive_failures"], 0);
        server.reset().await;
        let target = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", target.uri()))
            .expect(1)
            .mount(&server)
            .await;
        assert!(webhook.send_test().await.is_err());
        assert!(target.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn disabled_event_gates_dedup_expiry_and_bounded_queue() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let webhook = configured(&server).await;
        webhook
            .update(
                serde_json::from_value(
                    json!({"event_quota_exceeded": false, "event_user_registered": false}),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        webhook.registered("a@test", RegistrationSource::Jit, false);
        webhook.quota_exceeded("a@test", 10, 10, 1, "imap.append");
        assert!(server.received_requests().await.unwrap().is_empty());
        webhook
            .update(serde_json::from_value(json!({"event_quota_exceeded": true})).unwrap())
            .await
            .unwrap();
        webhook.quota_exceeded("a@test", 10, 10, 1, "imap.append");
        webhook.quota_exceeded("a@test", 10, 10, 2, "imap.append");
        webhook.quota_exceeded("b@test", 10, 10, 2, "smtp.inbound");
        tokio::time::timeout(Duration::from_secs(2), async {
            while webhook.snapshot()["successful_deliveries"] != 2 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        webhook
            .inner
            .quota_dedup
            .lock()
            .unwrap()
            .insert("a@test".into(), Instant::now() - QUOTA_DEDUP);
        webhook.quota_exceeded("a@test", 10, 10, 1, "imap.append");
        tokio::time::timeout(Duration::from_secs(2), async {
            while webhook.snapshot()["successful_deliveries"] != 3 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        // An unstarted receiver proves the hot path is bounded and never awaits.
        let bounded = OperatorWebhooks::new(chatmail_db::init_memory_db().await.unwrap());
        bounded.replace(Settings {
            enabled: true,
            url: server.uri(),
            ..Settings::default()
        });
        for _ in 0..QUEUE_CAPACITY + 1 {
            bounded.registered("a@test", RegistrationSource::Admin, false);
        }
        assert_eq!(bounded.snapshot()["dropped_events"], 1);
        let old_generation = bounded.inner.config.read().unwrap().generation;
        bounded.replace(Settings::default());
        assert!(bounded
            .deliver(&Job {
                body: b"{}".to_vec(),
                generation: old_generation
            })
            .await
            .is_err());
    }

    #[tokio::test]
    async fn config_survives_reload_and_new_runtime_and_rejects_invalid_update() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let webhook = configured(&server).await;
        assert!(webhook
            .update(serde_json::from_value(json!({"url": "http://remote.example"})).unwrap())
            .await
            .is_err());
        assert_eq!(webhook.snapshot()["url"], format!("{}/hook", server.uri()));
        webhook.hydrate().await.unwrap();
        webhook.send_test().await.unwrap();
        let fresh = OperatorWebhooks::new(webhook.inner.pool.clone());
        fresh.hydrate().await.unwrap();
        assert_eq!(fresh.snapshot()["secret_configured"], true);
        fresh.send_test().await.unwrap();
        assert!(!fresh.snapshot().to_string().contains("test-only-secret"));
    }
    #[tokio::test]
    async fn request_timeout_and_unsigned_delivery() {
        let server = MockServer::start().await;
        let webhook = configured(&server).await;
        webhook
            .update(serde_json::from_value(json!({"timeout_seconds": 1, "secret": ""})).unwrap())
            .await
            .unwrap();
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(3)))
            .mount(&server)
            .await;
        let start = Instant::now();
        assert!(webhook.send_test().await.is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        let requests = server.received_requests().await.unwrap();
        assert!(!requests[0].headers.contains_key("X-Madmail-Signature"));
        assert_eq!(webhook.snapshot()["consecutive_failures"], 1);
        webhook
            .update(serde_json::from_value(json!({"enabled": false})).unwrap())
            .await
            .unwrap();
        assert!(webhook.send_test().await.is_err());
        webhook.registered("disabled@test", RegistrationSource::Web, true);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn full_dedup_cache_preserves_one_hour_guarantee() {
        let server = MockServer::start().await;
        let webhook = configured(&server).await;
        {
            let mut dedup = webhook.inner.quota_dedup.lock().unwrap();
            for index in 0..DEDUP_CAPACITY {
                dedup.insert(format!("user{index}@test"), Instant::now());
            }
        }
        webhook.quota_exceeded("new@test", 1, 1, 1, "imap.append");
        webhook.quota_exceeded("user0@test", 1, 1, 1, "imap.append");
        assert_eq!(webhook.snapshot()["dropped_events"], 1);
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
