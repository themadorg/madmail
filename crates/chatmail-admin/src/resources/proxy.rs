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

//! Shadowsocks / HTTP proxy admin (Madmail `/admin/services/shadowsocks`, settings).

use serde::Deserialize;
use serde_json::{json, Value};

use chatmail_db::{get_bool_setting, set_setting};
use chatmail_shadowsocks::resolve_runtime;

use super::settings::generic_setting;
use super::status_storage::db_err;
use super::toggles::trigger_soft_reload;
use super::AdminResult;
use crate::AdminState;

pub const HTTP_PROXY_NOT_IMPLEMENTED: &str = "HTTP proxy is not implemented in madmail-v2";

const SS_NOT_CONFIGURED: &str =
    "Shadowsocks is not configured in maddy.conf (set ss_addr and ss_password in the chatmail block)";

const SS_TRANSPORT_UNAVAILABLE: &str =
    "WebSocket and gRPC Shadowsocks transports are disabled; use raw TCP Shadowsocks only";

/// Setting names routed to the HTTP proxy stub.
pub const PROXY_SETTING_NAMES: &[&str] = &[
    "http_proxy_port",
    "http_proxy_path",
    "http_proxy_username",
    "http_proxy_password",
];

/// `GET/POST /admin/services/ss_ws` and `ss_grpc` — always disabled (raw TCP only).
pub async fn proxy_transport_disabled(method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => Ok((200, Some(json!({ "status": "disabled" })))),
        "POST" => {
            let req: ActionBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, format!("invalid body: {e}")))?;
            match req.action.to_ascii_lowercase().as_str() {
                "enable" => Err((400, SS_TRANSPORT_UNAVAILABLE.into())),
                "disable" => Ok((200, Some(json!({ "status": "disabled" })))),
                _ => Err((400, "action must be enable or disable".into())),
            }
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

/// `GET /admin/services/shadowsocks`.
pub async fn proxy_service(
    st: &AdminState,
    method: &str,
    body: &Value,
    db_key: &str,
) -> AdminResult {
    if !st.file_config.ss_configured() {
        return ss_not_configured_service(method, body, db_key).await;
    }
    proxy_toggle_service(st, method, body, db_key).await
}

async fn ss_not_configured_service(method: &str, body: &Value, db_key: &str) -> AdminResult {
    match method {
        "GET" => Ok((200, Some(json!({ "status": "disabled" })))),
        "POST" => {
            let req: ActionBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, format!("invalid body: {e}")))?;
            match req.action.to_ascii_lowercase().as_str() {
                "enable" => Err((400, SS_NOT_CONFIGURED.into())),
                "disable" => {
                    let _ = db_key;
                    Ok((200, Some(json!({ "status": "disabled" }))))
                }
                _ => Err((400, "action must be enable or disable".into())),
            }
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

async fn proxy_toggle_service(
    st: &AdminState,
    method: &str,
    body: &Value,
    db_key: &str,
) -> AdminResult {
    match method {
        "GET" => {
            let on = get_bool_setting(&st.pool, db_key, true)
                .await
                .map_err(db_err)?;
            let status = if on { "enabled" } else { "disabled" };
            Ok((200, Some(json!({ "status": status }))))
        }
        "POST" => {
            let req: ActionBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, format!("invalid body: {e}")))?;
            let on = match req.action.to_ascii_lowercase().as_str() {
                "enable" => true,
                "disable" => false,
                _ => return Err((400, "action must be enable or disable".into())),
            };
            set_setting(&st.pool, db_key, if on { "true" } else { "false" })
                .await
                .map_err(db_err)?;
            let status = if on { "enabled" } else { "disabled" };
            Ok((
                200,
                Some(json!({
                    "status": status,
                    "restart_required": st.reload_tx.is_some(),
                })),
            ))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

/// `GET`/`POST` on `/admin/settings/ss_port`, etc. (SS) or HTTP proxy stubs.
pub async fn proxy_setting(st: &AdminState, method: &str, body: &Value, name: &str) -> AdminResult {
    if PROXY_SETTING_NAMES.contains(&name) {
        return http_proxy_setting(method).await;
    }
    ss_named_setting(st, method, body, name).await
}

async fn http_proxy_setting(method: &str) -> AdminResult {
    match method {
        "GET" | "POST" => Err((400, HTTP_PROXY_NOT_IMPLEMENTED.into())),
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

async fn ss_named_setting(st: &AdminState, method: &str, body: &Value, name: &str) -> AdminResult {
    if !st.file_config.ss_configured() {
        return Err((400, SS_NOT_CONFIGURED.into()));
    }
    use chatmail_db::settings_keys as k;
    let db_key = match name {
        "ss_port" => k::SS_PORT,
        "ss_ws_port" => k::SS_WS_PORT,
        "ss_grpc_port" => k::SS_GRPC_PORT,
        "ss_cipher" => k::SS_CIPHER,
        "ss_password" => k::SS_PASSWORD,
        _ => return Err((404, format!("unknown setting: {name}"))),
    };
    let mut res = generic_setting(st, method, body, db_key).await?;
    if method == "POST" && st.reload_tx.is_some() {
        trigger_soft_reload(st).await?;
        if let Some(ref mut body) = res.1 {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("restart_required".into(), json!(true));
            }
        }
    }
    Ok(res)
}

/// `GET /admin/services/http_proxy` — not implemented.
pub async fn http_proxy_service(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    let _ = st;
    match method {
        "GET" => Ok((200, Some(json!({ "status": "disabled" })))),
        "POST" => {
            let req: ActionBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, format!("invalid body: {e}")))?;
            match req.action.to_ascii_lowercase().as_str() {
                "enable" => Err((400, HTTP_PROXY_NOT_IMPLEMENTED.into())),
                "disable" => Ok((200, Some(json!({ "status": "disabled" })))),
                _ => Err((400, "action must be enable or disable".into())),
            }
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

/// Shadowsocks snapshot for `GET /admin/settings`.
pub async fn shadowsocks_settings_snapshot(
    st: &AdminState,
) -> Result<(String, String, String, String, String, String, String), (u16, String)> {
    if !st.file_config.ss_configured() {
        return Ok((
            "disabled".into(),
            "disabled".into(),
            "disabled".into(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ));
    }
    let rt = resolve_runtime(&st.pool, &st.file_config, &st.mail_domain, &st.state_dir)
        .await
        .map_err(db_err)?;

    let ss_enabled = if rt.enabled { "enabled" } else { "disabled" };
    let ss_ws = "disabled";
    let ss_grpc = "disabled";
    let urls = rt.urls(&st.mail_domain);
    let (_, port) = rt.listen_addr.rsplit_once(':').unwrap_or(("", ""));
    Ok((
        ss_enabled.to_string(),
        ss_ws.to_string(),
        ss_grpc.to_string(),
        port.to_string(),
        rt.cipher,
        rt.password,
        urls.shadowsocks_url,
    ))
}

#[derive(Deserialize)]
struct ActionBody {
    action: String,
}
