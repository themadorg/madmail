// Copyright (C) 2026 themadorg
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::Deserialize;
use serde_json::{json, Value};

use chatmail_db::{
    federation_policy_label, get_bool_setting, set_federation_policy_label, set_setting,
    settings_keys,
};
use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

#[derive(Deserialize)]
struct FederationPost {
    enabled: Option<bool>,
    policy: Option<String>,
}

#[derive(Deserialize)]
struct DomainBody {
    domain: String,
}

async fn federation_settings_body(st: &AdminState) -> Result<Value, (u16, String)> {
    let enabled =
        get_bool_setting(&st.pool, settings_keys::FEDERATION_ENABLED, false)
            .await
            .map_err(db_err)?;
    let policy = federation_policy_label(&st.pool).await.map_err(db_err)?;
    Ok(json!({
        "enabled": enabled,
        "policy": policy,
    }))
}

pub async fn policy(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => Ok((200, Some(federation_settings_body(st).await?))),
        "POST" => {
            let req: FederationPost = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            if let Some(en) = req.enabled {
                set_setting(
                    &st.pool,
                    settings_keys::FEDERATION_ENABLED,
                    if en { "true" } else { "false" },
                )
                .await
                .map_err(db_err)?;
            }
            if let Some(p) = req.policy {
                set_federation_policy_label(&st.pool, &p)
                    .await
                    .map_err(db_err)?;
            }
            Ok((200, Some(federation_settings_body(st).await?)))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

pub async fn rules(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => {
            let rows = st
                .app
                .federation_policy
                .list_rules(&st.pool)
                .await
                .map_err(db_err)?;
            let rules: Vec<_> = rows
                .into_iter()
                .map(|(domain, created_at)| json!({ "domain": domain, "created_at": created_at }))
                .collect();
            Ok((200, Some(json!({ "rules": rules, "total": rules.len() }))))
        }
        "POST" => {
            let req: DomainBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            if req.domain.trim().is_empty() {
                return Err((400, "domain is required".into()));
            }
            st.app
                .federation_policy
                .add_rule(&st.pool, &req.domain)
                .await
                .map_err(db_err)?;
            let total = st.app.federation_policy.list_exceptions().len();
            Ok((200, Some(json!({ "domain": req.domain, "total": total }))))
        }
        "DELETE" => {
            let req: DomainBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            st.app
                .federation_policy
                .remove_rule(&st.pool, &req.domain)
                .await
                .map_err(db_err)?;
            let remaining = st.app.federation_policy.list_exceptions().len();
            Ok((
                200,
                Some(json!({ "domain": req.domain, "remaining": remaining })),
            ))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

pub async fn silent_dismiss(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => {
            let rows = st
                .app
                .federation_silent_dismiss
                .list_rules(&st.pool)
                .await
                .map_err(db_err)?;
            let domains: Vec<_> = rows
                .into_iter()
                .map(|(domain, created_at)| json!({ "domain": domain, "created_at": created_at }))
                .collect();
            Ok((
                200,
                Some(json!({ "domains": domains, "total": domains.len() })),
            ))
        }
        "POST" => {
            let req: DomainBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            if req.domain.trim().is_empty() {
                return Err((400, "domain is required".into()));
            }
            st.app
                .federation_silent_dismiss
                .add(&st.pool, &req.domain)
                .await
                .map_err(db_err)?;
            let total = st.app.federation_silent_dismiss.list_domains().len();
            Ok((200, Some(json!({ "domain": req.domain, "total": total }))))
        }
        "DELETE" => {
            let req: DomainBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            if req.domain.trim().is_empty() {
                return Err((400, "domain is required".into()));
            }
            st.app
                .federation_silent_dismiss
                .remove(&st.pool, &req.domain)
                .await
                .map_err(db_err)?;
            let remaining = st.app.federation_silent_dismiss.list_domains().len();
            Ok((
                200,
                Some(json!({ "domain": req.domain, "remaining": remaining })),
            ))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

pub async fn servers(st: &AdminState, method: &str) -> AdminResult {
    if method != "GET" {
        return Err((405, "use GET".into()));
    }
    let snap = st.app.federation_tracker.snapshot();
    let servers: Vec<_> = snap
        .into_iter()
        .map(|s| {
            let mean_latency_ms = if s.successful_deliveries > 0 {
                s.total_latency_ms as f64 / s.successful_deliveries as f64
            } else {
                0.0
            };
            json!({
                "domain": s.domain,
                "queued_messages": s.queued_messages,
                "failed_http": s.failed_http,
                "failed_https": s.failed_https,
                "failed_smtp": s.failed_smtp,
                "success_http": s.success_http,
                "success_https": s.success_https,
                "success_smtp": s.success_smtp,
                "inbound_deliveries": s.inbound_deliveries,
                "successful_deliveries": s.successful_deliveries,
                "mean_latency_ms": mean_latency_ms,
                "last_active": s.last_active,
            })
        })
        .collect();
    Ok((
        200,
        Some(json!({ "servers": servers, "total": servers.len() })),
    ))
}
