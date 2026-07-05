// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chatmail_db::{get_bool_setting, get_setting, set_setting, settings_keys, DbPool};
use chatmail_types::{ChatmailError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEvent {
    UserRegistered,
    QuotaExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secret: String,
    pub event_user_registered: bool,
    pub event_quota_exceeded: bool,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            secret: String::new(),
            event_user_registered: true,
            event_quota_exceeded: true,
        }
    }
}

impl WebhookConfig {
    pub fn validates_url(url: &str) -> Result<()> {
        let url = url.trim();
        if url.is_empty() {
            return Err(ChatmailError::config("webhook url required when enabled"));
        }
        if url.starts_with("https://") {
            return Ok(());
        }
        // Local mock servers (tests / dev only).
        if url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost") {
            return Ok(());
        }
        Err(ChatmailError::config(
            "webhook url must use https:// (http allowed only for localhost)",
        ))
    }

    pub fn validate(&self) -> Result<()> {
        if self.enabled {
            Self::validates_url(&self.url)?;
        }
        Ok(())
    }

    pub fn wants(&self, event: WebhookEvent) -> bool {
        if !self.enabled || self.url.is_empty() {
            return false;
        }
        match event {
            WebhookEvent::UserRegistered => self.event_user_registered,
            WebhookEvent::QuotaExceeded => self.event_quota_exceeded,
        }
    }
}

pub async fn webhook_config(pool: &DbPool) -> Result<WebhookConfig> {
    let enabled = get_bool_setting(pool, settings_keys::WEBHOOK_ENABLED, false).await?;
    let url = get_setting(pool, settings_keys::WEBHOOK_URL)
        .await?
        .unwrap_or_default();
    let secret = get_setting(pool, settings_keys::WEBHOOK_SECRET)
        .await?
        .unwrap_or_default();
    let event_user_registered =
        get_bool_setting(pool, settings_keys::WEBHOOK_EVENT_USER_REGISTERED, true).await?;
    let event_quota_exceeded =
        get_bool_setting(pool, settings_keys::WEBHOOK_EVENT_QUOTA_EXCEEDED, true).await?;
    Ok(WebhookConfig {
        enabled,
        url,
        secret,
        event_user_registered,
        event_quota_exceeded,
    })
}

pub async fn save_webhook_config(pool: &DbPool, config: &WebhookConfig) -> Result<()> {
    config.validate()?;
    set_setting(
        pool,
        settings_keys::WEBHOOK_ENABLED,
        if config.enabled { "true" } else { "false" },
    )
    .await?;
    set_setting(pool, settings_keys::WEBHOOK_URL, config.url.trim()).await?;
    if !config.secret.is_empty() {
        set_setting(pool, settings_keys::WEBHOOK_SECRET, config.secret.trim()).await?;
    }
    set_setting(
        pool,
        settings_keys::WEBHOOK_EVENT_USER_REGISTERED,
        if config.event_user_registered {
            "true"
        } else {
            "false"
        },
    )
    .await?;
    set_setting(
        pool,
        settings_keys::WEBHOOK_EVENT_QUOTA_EXCEEDED,
        if config.event_quota_exceeded {
            "true"
        } else {
            "false"
        },
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_https_url() {
        assert!(WebhookConfig::validates_url("http://example.com/hook").is_err());
        assert!(WebhookConfig::validates_url("https://example.com/hook").is_ok());
    }

    #[tokio::test]
    async fn roundtrip_settings() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let cfg = WebhookConfig {
            enabled: true,
            url: "https://hooks.example.com/madmail".into(),
            secret: "s3cret".into(),
            event_user_registered: true,
            event_quota_exceeded: false,
        };
        save_webhook_config(&pool, &cfg).await.unwrap();
        let loaded = webhook_config(&pool).await.unwrap();
        assert_eq!(loaded.enabled, true);
        assert_eq!(loaded.url, "https://hooks.example.com/madmail");
        assert_eq!(loaded.secret, "s3cret");
        assert!(!loaded.event_quota_exceeded);
    }
}