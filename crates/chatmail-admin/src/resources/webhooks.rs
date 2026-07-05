// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::Deserialize;
use serde_json::{json, Value};

use chatmail_webhooks::{save_webhook_config, webhook_config};

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

#[derive(Deserialize, Default)]
struct WebhooksPut {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    event_user_registered: Option<bool>,
    #[serde(default)]
    event_quota_exceeded: Option<bool>,
}

#[derive(Deserialize)]
struct ActionBody {
    action: String,
}

async fn config_json(st: &AdminState) -> AdminResult {
    let cfg = webhook_config(&st.pool).await.map_err(db_err)?;
    let stats = st.app.webhooks.stats();
    Ok((
        200,
        Some(json!({
            "enabled": cfg.enabled,
            "url": cfg.url,
            "secret_configured": !cfg.secret.is_empty(),
            "event_user_registered": cfg.event_user_registered,
            "event_quota_exceeded": cfg.event_quota_exceeded,
            "successful_deliveries": stats.successful_deliveries.load(std::sync::atomic::Ordering::Relaxed),
            "consecutive_failures": stats.consecutive_failures.load(std::sync::atomic::Ordering::Relaxed),
        })),
    ))
}

pub async fn service(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => config_json(st).await,
        "PUT" => {
            let req: WebhooksPut =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
            let mut cfg = webhook_config(&st.pool).await.map_err(db_err)?;
            if let Some(v) = req.enabled {
                cfg.enabled = v;
            }
            if let Some(v) = req.url {
                cfg.url = v;
            }
            if let Some(v) = req.secret {
                if !v.is_empty() {
                    cfg.secret = v;
                }
            }
            if let Some(v) = req.event_user_registered {
                cfg.event_user_registered = v;
            }
            if let Some(v) = req.event_quota_exceeded {
                cfg.event_quota_exceeded = v;
            }
            save_webhook_config(&st.pool, &cfg).await.map_err(db_err)?;
            st.app.webhooks.invalidate_config_cache();
            config_json(st).await
        }
        "POST" => {
            let req: ActionBody =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
            if req.action.to_ascii_lowercase() != "test" {
                return Err((400, "action must be test".into()));
            }
            st.app
                .webhooks
                .send_test()
                .await
                .map_err(|e| (502, e))?;
            Ok((200, Some(json!({ "status": "ok", "message": "test webhook delivered" }))))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}