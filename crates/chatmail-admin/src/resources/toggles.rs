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

use chatmail_db::{get_bool_setting, set_setting, settings_keys, DbPool};

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

#[derive(Deserialize)]
struct ActionBody {
    action: String,
}

pub async fn registration(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    registration_toggle(&st.pool, method, body).await
}

pub async fn jit(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    toggle_setting(
        &st.pool,
        method,
        body,
        settings_keys::JIT_REGISTRATION_ENABLED,
        "enabled",
        "disabled",
    )
    .await
}

pub async fn service_bool(
    st: &AdminState,
    method: &str,
    body: &Value,
    key: &str,
) -> AdminResult {
    let mut res = toggle_setting(
        &st.pool,
        method,
        body,
        key,
        "enabled",
        "disabled",
    )
    .await?;
    if method == "POST" && key == chatmail_db::settings_keys::ADMIN_WEB_ENABLED {
        if let Some(body) = &res.1 {
            if body.get("status").and_then(|v| v.as_str()) == Some("enabled") {
                chatmail_admin_web::ensure_default_admin_web_path(&st.file_config, &st.pool)
                    .await
                    .map_err(|e| (500, e.to_string()))?;
            }
        }
    }
    if method == "POST"
        && (key == chatmail_db::settings_keys::TURN_ENABLED
            || key == chatmail_db::settings_keys::IROH_ENABLED
            || key == chatmail_db::settings_keys::ADMIN_WEB_ENABLED)
    {
        if let Some(body) = &mut res.1 {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("restart_required".into(), json!(true));
            }
        }
    }
    Ok(res)
}

/// Queue supervisor soft reload (TURN relay + IMAP METADATA).
pub(crate) async fn trigger_soft_reload(st: &AdminState) -> Result<(), (u16, String)> {
    let Some(tx) = &st.reload_tx else {
        return Err((
            501,
            "soft reload not available (server started without reload channel)".into(),
        ));
    };
    tx.try_send(()).map_err(|_| {
        (
            409,
            "reload already in progress; wait for the current reload to finish".into(),
        )
    })?;
    Ok(())
}

/// Madmail `RegistrationHandler` — actions `open` / `close`.
async fn registration_toggle(pool: &DbPool, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => {
            let open = get_bool_setting(pool, settings_keys::REGISTRATION_OPEN, false)
                .await
                .map_err(db_err)?;
            let status = if open { "open" } else { "closed" };
            Ok((200, Some(json!({ "status": status }))))
        }
        "POST" => {
            let req: ActionBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, format!("invalid body: {e}")))?;
            let (open, status) = match req.action.as_str() {
                "open" => (true, "open"),
                "close" => (false, "closed"),
                _ => {
                    return Err((
                        400,
                        format!("invalid action: {} (expected open|close)", req.action),
                    ));
                }
            };
            set_setting(
                pool,
                settings_keys::REGISTRATION_OPEN,
                if open { "true" } else { "false" },
            )
            .await
            .map_err(db_err)?;
            Ok((200, Some(json!({ "status": status }))))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

/// Madmail service toggles — actions `enable` / `disable`.
async fn toggle_setting(
    pool: &DbPool,
    method: &str,
    body: &Value,
    key: &str,
    on_label: &str,
    off_label: &str,
) -> AdminResult {
    match method {
        "GET" => {
            let on = get_bool_setting(pool, key, false).await.map_err(db_err)?;
            Ok((200, Some(json!({ "status": if on { on_label } else { off_label } }))))
        }
        "POST" => {
            let req: ActionBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, format!("invalid body: {e}")))?;
            let on = match req.action.to_ascii_lowercase().as_str() {
                "enable" => true,
                "disable" => false,
                _ => return Err((400, "action must be enable or disable".into())),
            };
            set_setting(pool, key, if on { "true" } else { "false" })
                .await
                .map_err(db_err)?;
            Ok((
                200,
                Some(json!({
                    "status": if on { on_label } else { off_label },
                })),
            ))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

