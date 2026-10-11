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

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;
use chatmail_db::{
    federation_policy_label, get_bool_setting, get_setting, set_federation_policy_label,
    set_setting, settings_keys,
};
use chatmail_state::{
    domain_overrides_json, load_domain_overrides, load_transport_policy, parse_override_mode,
    parse_transport_mode, set_transport_mode, update_domain_transports, PolicyMode, TransportName,
    TransportPolicy,
};
use chatmail_types::ChatmailError;

#[derive(Deserialize)]
struct TransportPost {
    https: Option<String>,
    http: Option<String>,
    smtp: Option<String>,
}

#[derive(Deserialize)]
struct FederationPost {
    enabled: Option<bool>,
    policy: Option<String>,
    /// When set, `transports` is a per-destination override. `inherit` clears one mode.
    domain: Option<String>,
    transports: Option<TransportPost>,
}

#[derive(Deserialize)]
struct DomainBody {
    domain: String,
}

async fn federation_settings_body(st: &AdminState) -> Result<Value, (u16, String)> {
    use chatmail_config::format_data_size;

    let enabled = get_bool_setting(&st.pool, settings_keys::FEDERATION_ENABLED, false)
        .await
        .map_err(db_err)?;
    let policy = federation_policy_label(&st.pool).await.map_err(db_err)?;
    let max_federation_size = get_setting(&st.pool, settings_keys::MAX_FEDERATION_SIZE)
        .await
        .map_err(db_err)?;
    let federation_effective = st.app.federation_size.effective();
    let transports = load_transport_policy(&st.pool).await.map_err(db_err)?;
    let overrides = load_domain_overrides(&st.pool).await.map_err(db_err)?;
    Ok(json!({
        "enabled": enabled,
        "policy": policy,
        "max_federation_size": max_federation_size,
        "federation_size_effective": format_data_size(federation_effective),
        "federation_size_effective_bytes": federation_effective,
        "transports": transport_json(transports),
        "transport_overrides": domain_overrides_json(&overrides),
    }))
}

fn transport_json(policy: TransportPolicy) -> Value {
    json!({
        "https": policy.mode(TransportName::Https),
        "http": policy.mode(TransportName::Http),
        "smtp": policy.mode(TransportName::Smtp),
    })
}

fn transport_fields(body: TransportPost) -> [(TransportName, Option<String>); 3] {
    [
        (TransportName::Https, body.https),
        (TransportName::Http, body.http),
        (TransportName::Smtp, body.smtp),
    ]
}

fn config_or_db(err: ChatmailError) -> (u16, String) {
    match err {
        ChatmailError::Config(msg) => (400, msg),
        other => db_err(other),
    }
}

async fn apply_transport_post(
    st: &AdminState,
    body: TransportPost,
    domain: Option<String>,
) -> Result<(), (u16, String)> {
    if let Some(domain) = domain {
        if domain.trim().is_empty() {
            return Err((400, "domain is required".into()));
        }
        let mut parsed = Vec::new();
        for (name, mode) in transport_fields(body) {
            let Some(mode) = mode else { continue };
            let mode = parse_override_mode(&mode).map_err(|e| (400, e.to_string()))?;
            parsed.push((name, mode));
        }
        if parsed.is_empty() {
            return Err((400, "transports required when domain is set".into()));
        }
        update_domain_transports(&st.pool, &domain, &parsed)
            .await
            .map_err(config_or_db)?;
    } else {
        let mut parsed = Vec::new();
        for (name, mode) in transport_fields(body) {
            let Some(mode) = mode else { continue };
            let allow = parse_transport_mode(&mode).map_err(|e| (400, e.to_string()))?;
            parsed.push((name, allow));
        }
        for (name, allow) in parsed {
            set_transport_mode(&st.pool, name, allow)
                .await
                .map_err(db_err)?;
        }
    }
    st.app
        .federation_transport
        .hydrate(&st.pool)
        .await
        .map_err(db_err)?;
    Ok(())
}

pub async fn policy(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => Ok((200, Some(federation_settings_body(st).await?))),
        "POST" => {
            let req: FederationPost =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
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
                st.app
                    .federation_policy
                    .set_global_mode(PolicyMode::from_label(&p));
            }
            if let Some(transports) = req.transports {
                apply_transport_post(st, transports, req.domain).await?;
            } else if req.domain.is_some() {
                return Err((400, "transports required when domain is set".into()));
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
            let req: DomainBody =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
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
            let req: DomainBody =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
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
            let req: DomainBody =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
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
            let req: DomainBody =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
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
