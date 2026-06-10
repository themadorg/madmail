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

//! Admin settings — Madmail `AllSettingsHandler` + `GenericSettingHandler`.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{json, Value};

use chatmail_config::{format_data_size, parse_duration};
use chatmail_db::{
    delete_setting, format_retention_days, get_bool_setting, get_setting, set_setting,
    settings_keys, DbPool, DEFAULT_RETENTION_DAYS,
};

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

/// Build the full settings snapshot (admin-web `AllSettings` type).
pub async fn all_settings(st: &AdminState, method: &str) -> AdminResult {
    if method != "GET" {
        return Err((405, "use GET".into()));
    }

    let pool = &st.pool;

    let registration = if get_bool_setting(pool, settings_keys::REGISTRATION_OPEN, false)
        .await
        .map_err(db_err)?
    {
        "open"
    } else {
        "closed"
    };

    let jit_registration = if get_bool_setting(pool, settings_keys::JIT_REGISTRATION_ENABLED, true)
        .await
        .map_err(db_err)?
    {
        "enabled"
    } else {
        "disabled"
    };

    let turn_enabled = if get_bool_setting(pool, settings_keys::TURN_ENABLED, true)
        .await
        .map_err(db_err)?
    {
        "enabled"
    } else {
        "disabled"
    };

    let federation_enabled = get_bool_setting(pool, settings_keys::FEDERATION_ENABLED, false)
        .await
        .map_err(db_err)?;

    let federation_policy = chatmail_db::federation_policy_label(pool)
        .await
        .map_err(db_err)?;

    let (ss_enabled, ss_ws_enabled, ss_grpc_enabled, ss_port, ss_cipher, ss_pass, shadowsocks_url) =
        super::proxy::shadowsocks_settings_snapshot(st).await?;

    let mut body = serde_json::Map::new();
    body.insert("registration".into(), json!(registration));
    body.insert("jit_registration".into(), json!(jit_registration));
    body.insert("turn_enabled".into(), json!(turn_enabled));
    body.insert(
        "iroh_enabled".into(),
        json!(get_toggle(pool, settings_keys::IROH_ENABLED, true).await?),
    );
    body.insert(
        "push_enabled".into(),
        json!(get_toggle(pool, settings_keys::PUSH_ENABLED, false).await?),
    );
    let push_mode = chatmail_push::push_mode(pool).await.map_err(db_err)?;
    body.insert("push_mode".into(), json!(push_mode.as_str()));
    body.insert("ss_enabled".into(), json!(ss_enabled));
    body.insert(
        "auto_purge_seen_enabled".into(),
        json!(get_toggle_disabled_default(pool, settings_keys::AUTO_PURGE_SEEN).await?),
    );
    body.insert(
        "message_retention_enabled".into(),
        json!(get_toggle_disabled_default(pool, settings_keys::MESSAGE_RETENTION_ENABLED).await?),
    );
    insert_setting(
        &mut body,
        "message_retention",
        setting_value(
            pool,
            settings_keys::MESSAGE_RETENTION,
            &format_retention_days(DEFAULT_RETENTION_DAYS),
        )
        .await?,
    );
    body.insert(
        "admin_web_enabled".into(),
        json!(get_toggle_disabled_default(pool, settings_keys::ADMIN_WEB_ENABLED).await?),
    );
    body.insert(
        "webimap_enabled".into(),
        json!(get_toggle_disabled_default(pool, settings_keys::WEBIMAP_ENABLED).await?),
    );
    body.insert(
        "websmtp_enabled".into(),
        json!(get_toggle_disabled_default(pool, settings_keys::WEBSMTP_ENABLED).await?),
    );
    body.insert(
        "registration_token_required".into(),
        json!(get_toggle_disabled_default(pool, settings_keys::REGISTRATION_TOKEN_REQUIRED).await?),
    );
    body.insert("federation_enabled".into(), json!(federation_enabled));
    body.insert("federation_policy".into(), json!(federation_policy));

    insert_setting(
        &mut body,
        "smtp_port",
        setting_value(pool, settings_keys::SMTP_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "submission_port",
        setting_value(pool, settings_keys::SUBMISSION_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "submission_tls_port",
        setting_value(pool, settings_keys::SUBMISSION_TLS_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "imap_port",
        setting_value(pool, settings_keys::IMAP_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "imap_tls_port",
        setting_value(pool, settings_keys::IMAP_TLS_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "turn_port",
        setting_value(pool, settings_keys::TURN_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "sasl_port",
        setting_value(pool, settings_keys::SASL_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "iroh_port",
        setting_value(pool, settings_keys::IROH_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "ss_port",
        setting_value(pool, settings_keys::SS_PORT, &ss_port).await?,
    );
    insert_setting(
        &mut body,
        "ss_ws_port",
        setting_value(pool, settings_keys::SS_WS_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "ss_grpc_port",
        setting_value(pool, settings_keys::SS_GRPC_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "http_port",
        setting_value(pool, settings_keys::HTTP_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "https_port",
        setting_value(pool, settings_keys::HTTPS_PORT, "").await?,
    );

    body.insert(
        "smtp_access".into(),
        json!(port_access(pool, settings_keys::SMTP_LOCAL_ONLY).await?),
    );
    body.insert(
        "submission_access".into(),
        json!(port_access(pool, settings_keys::SUBMISSION_LOCAL_ONLY).await?),
    );
    body.insert(
        "submission_tls_access".into(),
        json!(port_access(pool, settings_keys::SUBMISSION_TLS_LOCAL_ONLY).await?),
    );
    body.insert(
        "imap_access".into(),
        json!(port_access(pool, settings_keys::IMAP_LOCAL_ONLY).await?),
    );
    body.insert(
        "imap_tls_access".into(),
        json!(port_access(pool, settings_keys::IMAP_TLS_LOCAL_ONLY).await?),
    );
    body.insert(
        "turn_access".into(),
        json!(port_access(pool, settings_keys::TURN_LOCAL_ONLY).await?),
    );
    body.insert(
        "sasl_access".into(),
        json!(port_access(pool, settings_keys::SASL_LOCAL_ONLY).await?),
    );
    body.insert(
        "iroh_access".into(),
        json!(port_access(pool, settings_keys::IROH_LOCAL_ONLY).await?),
    );
    body.insert(
        "http_access".into(),
        json!(port_access(pool, settings_keys::HTTP_LOCAL_ONLY).await?),
    );
    body.insert(
        "https_access".into(),
        json!(port_access(pool, settings_keys::HTTPS_LOCAL_ONLY).await?),
    );

    insert_setting(
        &mut body,
        "smtp_hostname",
        setting_value(pool, settings_keys::SMTP_HOSTNAME, "").await?,
    );
    insert_setting(
        &mut body,
        "turn_realm",
        setting_value(pool, settings_keys::TURN_REALM, "").await?,
    );
    insert_setting(
        &mut body,
        "turn_secret",
        setting_value(pool, settings_keys::TURN_SECRET, "").await?,
    );
    insert_setting(
        &mut body,
        "turn_relay_ip",
        setting_value(pool, settings_keys::TURN_RELAY_IP, "").await?,
    );
    insert_setting(
        &mut body,
        "turn_ttl",
        setting_value(pool, settings_keys::TURN_TTL, "").await?,
    );
    insert_setting(
        &mut body,
        "iroh_relay_url",
        setting_value(pool, settings_keys::IROH_RELAY_URL, "").await?,
    );
    insert_setting(
        &mut body,
        "ss_cipher",
        setting_value(pool, settings_keys::SS_CIPHER, &ss_cipher).await?,
    );
    insert_setting(
        &mut body,
        "ss_password",
        setting_value(pool, settings_keys::SS_PASSWORD, &ss_pass).await?,
    );
    body.insert("shadowsocks_url".into(), json!(shadowsocks_url));

    body.insert("ss_ws_enabled".into(), json!(ss_ws_enabled));
    body.insert("ss_grpc_enabled".into(), json!(ss_grpc_enabled));
    body.insert("http_proxy_enabled".into(), json!("disabled"));

    insert_setting(
        &mut body,
        "http_proxy_port",
        setting_value(pool, settings_keys::HTTP_PROXY_PORT, "").await?,
    );
    insert_setting(
        &mut body,
        "http_proxy_path",
        setting_value(pool, settings_keys::HTTP_PROXY_PATH, "").await?,
    );
    insert_setting(
        &mut body,
        "http_proxy_username",
        setting_value(pool, settings_keys::HTTP_PROXY_USERNAME, "").await?,
    );
    insert_setting(
        &mut body,
        "http_proxy_password",
        setting_value(pool, settings_keys::HTTP_PROXY_PASSWORD, "").await?,
    );

    insert_setting(
        &mut body,
        "admin_path",
        setting_value(pool, settings_keys::ADMIN_PATH, "").await?,
    );
    insert_setting(
        &mut body,
        "admin_web_path",
        setting_value(
            pool,
            settings_keys::ADMIN_WEB_PATH,
            chatmail_admin_web::DEFAULT_ADMIN_WEB_PATH,
        )
        .await?,
    );
    insert_setting(
        &mut body,
        "dclogin_imap_security",
        setting_value(pool, settings_keys::DCLOGIN_IMAP_SECURITY, "ssl").await?,
    );
    insert_setting(
        &mut body,
        "dclogin_smtp_security",
        setting_value(pool, settings_keys::DCLOGIN_SMTP_SECURITY, "ssl").await?,
    );
    insert_setting(
        &mut body,
        "language",
        setting_value(pool, settings_keys::LANGUAGE, "en").await?,
    );
    insert_setting(
        &mut body,
        "appendlimit",
        setting_value(pool, settings_keys::APPENDLIMIT, "").await?,
    );
    insert_setting(
        &mut body,
        "max_message_size",
        setting_value(pool, settings_keys::MAX_MESSAGE_SIZE, "").await?,
    );
    let effective = st.app.message_size.effective();
    insert_setting(
        &mut body,
        "message_size_effective",
        json!(format_data_size(effective)),
    );
    insert_setting(&mut body, "message_size_effective_bytes", json!(effective));

    Ok((200, Some(Value::Object(body))))
}

fn insert_setting(map: &mut serde_json::Map<String, Value>, key: &str, value: Value) {
    map.insert(key.into(), value);
}

/// Toggle with default `"enabled"` when key missing (`getToggle` in Go).
async fn get_toggle(pool: &DbPool, key: &str, default_on: bool) -> Result<String, (u16, String)> {
    match get_setting(pool, key).await.map_err(db_err)? {
        None => Ok(if default_on {
            "enabled".into()
        } else {
            "disabled".into()
        }),
        Some(v) if v == "false" => Ok("disabled".into()),
        Some(_) => Ok("enabled".into()),
    }
}

/// Toggle with default `"disabled"` when key missing (`getToggleDisabledDefault` in Go).
async fn get_toggle_disabled_default(pool: &DbPool, key: &str) -> Result<String, (u16, String)> {
    match get_setting(pool, key).await.map_err(db_err)? {
        None => Ok("disabled".into()),
        Some(v) if v == "true" => Ok("enabled".into()),
        Some(_) => Ok("disabled".into()),
    }
}

/// `settingValueResponse` — `is_set` true only when stored in DB.
async fn setting_value(
    pool: &DbPool,
    key: &str,
    active_default: &str,
) -> Result<Value, (u16, String)> {
    match get_setting(pool, key).await.map_err(db_err)? {
        None => Ok(json!({
            "key": key,
            "value": active_default,
            "is_set": false,
            "restart_required": false
        })),
        Some(v) => Ok(json!({
            "key": key,
            "value": v,
            "is_set": true,
            "restart_required": false
        })),
    }
}

/// `"public"` unless `__*_LOCAL_ONLY__` is `"true"`.
async fn port_access(pool: &DbPool, key: &str) -> Result<String, (u16, String)> {
    let local = get_bool_setting(pool, key, false).await.map_err(db_err)?;
    Ok(if local { "local" } else { "public" }.into())
}

// ── `/admin/settings/{name}` — GenericSettingHandler ─────────────────────────

#[derive(Clone, Copy)]
enum NamedKind {
    /// GET/POST set|reset (`GenericSettingHandler`).
    Value,
    /// GET status + POST enable|disable (admin-web `setToggle` on `registration_token_required`).
    DbToggleDefaultDisabled,
}

struct NamedRoute {
    db_key: &'static str,
    kind: NamedKind,
}

fn named_routes() -> HashMap<&'static str, NamedRoute> {
    use settings_keys as k;
    let mut m = HashMap::new();
    let value = |db_key: &'static str| NamedRoute {
        db_key,
        kind: NamedKind::Value,
    };
    for (path, key) in [
        ("smtp_port", k::SMTP_PORT),
        ("submission_port", k::SUBMISSION_PORT),
        ("submission_tls_port", k::SUBMISSION_TLS_PORT),
        ("imap_port", k::IMAP_PORT),
        ("imap_tls_port", k::IMAP_TLS_PORT),
        ("turn_port", k::TURN_PORT),
        ("sasl_port", k::SASL_PORT),
        ("iroh_port", k::IROH_PORT),
        ("http_port", k::HTTP_PORT),
        ("https_port", k::HTTPS_PORT),
        ("http_proxy_port", k::HTTP_PROXY_PORT),
        ("smtp_local_only", k::SMTP_LOCAL_ONLY),
        ("submission_local_only", k::SUBMISSION_LOCAL_ONLY),
        ("submission_tls_local_only", k::SUBMISSION_TLS_LOCAL_ONLY),
        ("imap_local_only", k::IMAP_LOCAL_ONLY),
        ("imap_tls_local_only", k::IMAP_TLS_LOCAL_ONLY),
        ("turn_local_only", k::TURN_LOCAL_ONLY),
        ("iroh_local_only", k::IROH_LOCAL_ONLY),
        ("http_local_only", k::HTTP_LOCAL_ONLY),
        ("https_local_only", k::HTTPS_LOCAL_ONLY),
        ("smtp_hostname", k::SMTP_HOSTNAME),
        ("turn_realm", k::TURN_REALM),
        ("turn_secret", k::TURN_SECRET),
        ("turn_relay_ip", k::TURN_RELAY_IP),
        ("turn_ttl", k::TURN_TTL),
        ("iroh_relay_url", k::IROH_RELAY_URL),
        ("http_proxy_path", k::HTTP_PROXY_PATH),
        ("http_proxy_username", k::HTTP_PROXY_USERNAME),
        ("http_proxy_password", k::HTTP_PROXY_PASSWORD),
        ("admin_path", k::ADMIN_PATH),
        ("admin_web_path", k::ADMIN_WEB_PATH),
        ("dclogin_imap_security", k::DCLOGIN_IMAP_SECURITY),
        ("dclogin_smtp_security", k::DCLOGIN_SMTP_SECURITY),
        ("language", k::LANGUAGE),
        ("appendlimit", k::APPENDLIMIT),
        ("max_message_size", k::MAX_MESSAGE_SIZE),
        ("message_retention", k::MESSAGE_RETENTION),
    ] {
        m.insert(path, value(key));
    }
    m.insert(
        "registration_token_required",
        NamedRoute {
            db_key: k::REGISTRATION_TOKEN_REQUIRED,
            kind: NamedKind::DbToggleDefaultDisabled,
        },
    );
    m
}

/// `POST /admin/settings/language`, etc. (not `/admin/settings` or `/admin/settings/federation`).
pub async fn named_setting(
    st: &AdminState,
    method: &str,
    resource: &str,
    body: &Value,
) -> AdminResult {
    const PREFIX: &str = "/admin/settings/";
    let Some(name) = resource.strip_prefix(PREFIX) else {
        return Err((404, format!("unknown resource: {resource}")));
    };
    if name.is_empty() || name.contains('/') {
        return Err((404, format!("unknown resource: {resource}")));
    }

    let routes = named_routes();
    let Some(route) = routes.get(name) else {
        return Err((404, format!("unknown resource: {resource}")));
    };

    match route.kind {
        NamedKind::Value => generic_setting(st, method, body, route.db_key).await,
        NamedKind::DbToggleDefaultDisabled => {
            db_toggle_setting(st, method, body, route.db_key, false).await
        }
    }
}

#[derive(Deserialize)]
struct SettingActionBody {
    action: String,
    #[serde(default)]
    value: Value,
}

/// Madmail `GenericSettingHandler` — GET; POST `set` / `reset`.
pub(crate) async fn generic_setting(
    st: &AdminState,
    method: &str,
    body: &Value,
    db_key: &str,
) -> AdminResult {
    match method {
        "GET" => {
            let stored = get_setting(&st.pool, db_key).await.map_err(db_err)?;
            let (value, is_set) = match stored {
                None => (String::new(), false),
                Some(v) => (v, true),
            };
            Ok((200, Some(setting_response(db_key, &value, is_set, false))))
        }
        "POST" => {
            let req: SettingActionBody =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
            match req.action.as_str() {
                "set" => {
                    let mut value = body_value_as_string(&req.value);
                    if value.is_empty() {
                        return Err((400, "value is required for action 'set'".into()));
                    }
                    if db_key == chatmail_db::settings_keys::ADMIN_WEB_PATH {
                        value = chatmail_admin_web::normalize_admin_web_path(&value)
                            .ok_or((400, "admin web path must not be empty".into()))?;
                        set_setting(
                            &st.pool,
                            chatmail_db::settings_keys::ADMIN_WEB_ENABLED,
                            "true",
                        )
                        .await
                        .map_err(db_err)?;
                    }
                    validate_setting_value(db_key, &value)?;
                    set_setting(&st.pool, db_key, &value)
                        .await
                        .map_err(db_err)?;
                    super::message_size::refresh_message_size_after_setting(st, db_key).await;
                    if db_key == chatmail_db::settings_keys::ADMIN_WEB_PATH {
                        super::toggles::trigger_http_routes_reload(st).await?;
                    }
                    Ok((200, Some(setting_response(db_key, &value, true, true))))
                }
                "reset" => {
                    delete_setting(&st.pool, db_key).await.map_err(db_err)?;
                    super::message_size::refresh_message_size_after_setting(st, db_key).await;
                    if db_key == chatmail_db::settings_keys::ADMIN_WEB_PATH {
                        super::toggles::trigger_http_routes_reload(st).await?;
                    }
                    Ok((200, Some(setting_response(db_key, "", false, true))))
                }
                _ => Err((
                    400,
                    format!("invalid action: {} (expected set|reset)", req.action),
                )),
            }
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

/// Enable/disable toggle stored as `true`/`false` (admin-web `setToggle`).
async fn db_toggle_setting(
    st: &AdminState,
    method: &str,
    body: &Value,
    db_key: &str,
    default_enabled: bool,
) -> AdminResult {
    match method {
        "GET" => {
            let on = get_bool_setting(&st.pool, db_key, default_enabled)
                .await
                .map_err(db_err)?;
            let status = if on { "enabled" } else { "disabled" };
            Ok((200, Some(json!({ "status": status }))))
        }
        "POST" => {
            let req: SettingActionBody =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
            let on = match req.action.to_ascii_lowercase().as_str() {
                "enable" => true,
                "disable" => false,
                "set" => {
                    let v = body_value_as_string(&req.value);
                    matches!(v.as_str(), "true" | "1" | "yes" | "on" | "enabled")
                }
                _ => {
                    return Err((
                        400,
                        format!("invalid action: {} (expected enable|disable)", req.action),
                    ));
                }
            };
            set_setting(&st.pool, db_key, if on { "true" } else { "false" })
                .await
                .map_err(db_err)?;
            let status = if on { "enabled" } else { "disabled" };
            Ok((200, Some(json!({ "status": status }))))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

fn setting_response(key: &str, value: &str, is_set: bool, restart_required: bool) -> Value {
    json!({
        "key": key,
        "value": value,
        "is_set": is_set,
        "restart_required": restart_required
    })
}

fn body_value_as_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        _ => value.to_string(),
    }
}

fn validate_setting_value(key: &str, value: &str) -> Result<(), (u16, String)> {
    if value.contains('\0') || value.contains('\n') || value.contains('\r') {
        return Err((400, "value contains invalid characters".into()));
    }

    if is_port_key(key) {
        let port: u16 = value
            .parse()
            .map_err(|_| (400, "invalid port number: must be 1-65535".into()))?;
        if port == 0 {
            return Err((400, "invalid port number: must be 1-65535".into()));
        }
        return Ok(());
    }

    if key == settings_keys::TURN_TTL {
        let ttl: i64 = value
            .parse()
            .map_err(|_| (400, "invalid TTL: must be a positive integer".into()))?;
        if ttl < 1 {
            return Err((400, "invalid TTL: must be a positive integer".into()));
        }
        return Ok(());
    }

    if key == settings_keys::DCLOGIN_IMAP_SECURITY || key == settings_keys::DCLOGIN_SMTP_SECURITY {
        match value {
            "starttls" | "ssl" | "default" => return Ok(()),
            _ => {
                return Err((
                    400,
                    "invalid dclogin security mode: expected starttls|ssl|default".into(),
                ));
            }
        }
    }

    if key == settings_keys::APPENDLIMIT || key == settings_keys::MAX_MESSAGE_SIZE {
        return super::message_size::validate_message_size_value(value);
    }

    if key == settings_keys::MESSAGE_RETENTION {
        if parse_duration(value).is_err() {
            return Err((
                400,
                "invalid retention: use Go-style duration (e.g. 30d, 720h, 24h)".into(),
            ));
        }
        return Ok(());
    }

    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || ".:/@-_".contains(c))
    {
        return Err((400, "value contains disallowed characters".into()));
    }

    if value.len() > 253 {
        return Err((400, "value too long (max 253 characters)".into()));
    }

    Ok(())
}

fn is_port_key(key: &str) -> bool {
    matches!(
        key,
        settings_keys::SMTP_PORT
            | settings_keys::SUBMISSION_PORT
            | settings_keys::SUBMISSION_TLS_PORT
            | settings_keys::IMAP_PORT
            | settings_keys::IMAP_TLS_PORT
            | settings_keys::TURN_PORT
            | settings_keys::SASL_PORT
            | settings_keys::IROH_PORT
            | settings_keys::SS_PORT
            | settings_keys::HTTP_PORT
            | settings_keys::HTTPS_PORT
            | settings_keys::SS_WS_PORT
            | settings_keys::SS_GRPC_PORT
            | settings_keys::HTTP_PROXY_PORT
    )
}
