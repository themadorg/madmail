// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chatmail_db::DbPool;
use dashmap::DashMap;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde_json::{json, Value};
use sha2::Sha256;
use tokio::sync::Mutex;

use crate::settings::{webhook_config, WebhookConfig, WebhookEvent};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const QUOTA_DEDUP_SECS: i64 = 3600;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Default)]
pub struct DeliveryStats {
    pub successful_deliveries: AtomicU64,
    pub consecutive_failures: AtomicU64,
}

#[derive(Clone)]
pub struct WebhookDispatcher {
    pool: DbPool,
    client: Client,
    stats: Arc<DeliveryStats>,
    quota_dedup: Arc<DashMap<String, i64>>,
    /// Cached config refreshed on each emit (cheap DB reads).
    config_cache: Arc<Mutex<Option<WebhookConfig>>>,
}

impl std::fmt::Debug for WebhookDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookDispatcher").finish_non_exhaustive()
    }
}

impl WebhookDispatcher {
    pub fn new(pool: DbPool) -> Self {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("reqwest client");
        Self {
            pool,
            client,
            stats: Arc::new(DeliveryStats::default()),
            quota_dedup: Arc::new(DashMap::new()),
            config_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn stats(&self) -> Arc<DeliveryStats> {
        Arc::clone(&self.stats)
    }

    pub fn invalidate_config_cache(&self) {
        if let Ok(mut guard) = self.config_cache.try_lock() {
            *guard = None;
        }
    }

    /// Fire-and-forget operator webhook for a new account.
    pub fn emit_user_registered(
        &self,
        username: &str,
        source: &str,
        registration_token_used: bool,
    ) {
        let payload = json!({
            "event": "user.registered",
            "timestamp": unix_now(),
            "username": username,
            "source": source,
            "registration_token_used": registration_token_used,
        });
        self.spawn_delivery(WebhookEvent::UserRegistered, payload);
    }

    /// Fire-and-forget when storage quota blocks an incoming write (deduped per user/hour).
    pub fn emit_quota_exceeded(
        &self,
        username: &str,
        used_bytes: u64,
        max_bytes: u64,
        incoming_bytes: u64,
    ) {
        let now = unix_now();
        if let Some(prev) = self.quota_dedup.get(username) {
            if now.saturating_sub(*prev) < QUOTA_DEDUP_SECS {
                return;
            }
        }
        self.quota_dedup.insert(username.to_string(), now);

        let payload = json!({
            "event": "user.quota_exceeded",
            "timestamp": now,
            "username": username,
            "used_bytes": used_bytes,
            "max_bytes": max_bytes,
            "incoming_bytes": incoming_bytes,
        });
        self.spawn_delivery(WebhookEvent::QuotaExceeded, payload);
    }

    /// Synchronous test delivery for admin "send test" (uses current config from DB).
    pub async fn send_test(&self) -> Result<(), String> {
        let cfg = webhook_config(&self.pool).await.map_err(|e| e.to_string())?;
        if !cfg.enabled {
            return Err("webhooks disabled".into());
        }
        cfg.validate().map_err(|e| e.to_string())?;
        let payload = json!({
            "event": "webhook.test",
            "timestamp": unix_now(),
            "message": "madmail operator webhook test",
        });
        deliver(&self.client, &cfg, &payload)
            .await
            .map_err(|e| e.to_string())?;
        self.stats
            .successful_deliveries
            .fetch_add(1, Ordering::Relaxed);
        self.stats.consecutive_failures.store(0, Ordering::Relaxed);
        Ok(())
    }

    fn spawn_delivery(&self, event: WebhookEvent, payload: Value) {
        let this = self.clone();
        tokio::spawn(async move {
            let cfg = match this.load_config().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(error = %e, "webhook config load failed");
                    return;
                }
            };
            if !cfg.wants(event) {
                return;
            }
            match deliver(&this.client, &cfg, &payload).await {
                Ok(()) => {
                    this.stats
                        .successful_deliveries
                        .fetch_add(1, Ordering::Relaxed);
                    this.stats.consecutive_failures.store(0, Ordering::Relaxed);
                }
                Err(e) => {
                    this.stats
                        .consecutive_failures
                        .fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(event = ?event, error = %e, "webhook delivery failed");
                }
            }
        });
    }

    async fn load_config(&self) -> chatmail_types::Result<WebhookConfig> {
        let mut guard = self.config_cache.lock().await;
        if let Some(ref cfg) = *guard {
            return Ok(cfg.clone());
        }
        let cfg = webhook_config(&self.pool).await?;
        *guard = Some(cfg.clone());
        Ok(cfg)
    }
}

async fn deliver(client: &Client, cfg: &WebhookConfig, payload: &Value) -> Result<(), String> {
    let body = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    let mut req = client
        .post(cfg.url.trim())
        .header("Content-Type", "application/json")
        .body(body.clone());
    if !cfg.secret.is_empty() {
        let sig = sign_payload(&cfg.secret, &body)?;
        req = req.header("X-Madmail-Signature", format!("sha256={sig}"));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("HTTP {}", resp.status()))
    }
}

fn sign_payload(secret: &str, body: &[u8]) -> Result<String, String> {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|e| format!("hmac key: {e}"))?;
    mac.update(body);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn delivers_user_registered_payload() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/hook"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let pool = chatmail_db::init_memory_db().await.unwrap();
        crate::settings::save_webhook_config(
            &pool,
            &WebhookConfig {
                enabled: true,
                url: format!("{}/hook", server.uri()),
                secret: String::new(),
                event_user_registered: true,
                event_quota_exceeded: true,
            },
        )
        .await
        .unwrap();

        let d = WebhookDispatcher::new(pool);
        d.emit_user_registered("alice@example.org", "web", true);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(d.stats().successful_deliveries.load(Ordering::Relaxed) >= 1);
    }

    #[tokio::test]
    async fn signs_payload_when_secret_configured() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        use wiremock::matchers::header_exists;

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/hook"))
            .and(header_exists("X-Madmail-Signature"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let pool = chatmail_db::init_memory_db().await.unwrap();
        let secret = "test-secret";
        crate::settings::save_webhook_config(
            &pool,
            &WebhookConfig {
                enabled: true,
                url: format!("{}/hook", server.uri()),
                secret: secret.into(),
                event_user_registered: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let d = WebhookDispatcher::new(pool);
        d.emit_user_registered("alice@example.org", "admin", false);
        tokio::time::sleep(Duration::from_millis(200)).await;

        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1);
        let body = &received[0].body;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let expected = hex::encode(mac.finalize().into_bytes());
        let sig = received[0]
            .headers
            .get("X-Madmail-Signature")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(sig, format!("sha256={expected}"));
    }

    #[tokio::test]
    async fn quota_exceeded_deduped_within_hour() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/hook"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let pool = chatmail_db::init_memory_db().await.unwrap();
        crate::settings::save_webhook_config(
            &pool,
            &WebhookConfig {
                enabled: true,
                url: format!("{}/hook", server.uri()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let d = WebhookDispatcher::new(pool);
        d.emit_quota_exceeded("bob@example.org", 1000, 1000, 50);
        d.emit_quota_exceeded("bob@example.org", 1000, 1000, 50);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}