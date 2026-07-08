// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Enable WebIMAP + WebSMTP for local browser dev with a configurable CORS origin.

use serde::Deserialize;
use serde_json::{json, Value};

use chatmail_db::{get_setting, set_setting, settings_keys, DbPool};

fn parse_origins_list(raw: &str) -> Vec<String> {
    raw.split(&[',', '\n', '\r'][..])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn append_origin(existing: &str, origin: &str) -> String {
    let mut list = parse_origins_list(existing);
    if !list.iter().any(|o| o == origin) {
        list.push(origin.to_string());
    }
    list.join("\n")
}

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

#[derive(Deserialize)]
struct DevActionBody {
    action: String,
    #[serde(default)]
    origin: String,
}

pub async fn webmail_dev(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    let pool = &st.pool;
    match method {
        "GET" => {
            let cors = get_setting(pool, settings_keys::WEBMAIL_CORS_ORIGINS)
                .await
                .map_err(db_err)?
                .unwrap_or_default();
            let webimap = toggle_status(pool, settings_keys::WEBIMAP_ENABLED).await?;
            let websmtp = toggle_status(pool, settings_keys::WEBSMTP_ENABLED).await?;
            Ok((
                200,
                Some(json!({
                    "webimap_enabled": webimap,
                    "websmtp_enabled": websmtp,
                    "cors_origins": cors,
                    "cors_origins_list": parse_origins_list(&cors),
                })),
            ))
        }
        "POST" => {
            let req: DevActionBody =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
            match req.action.as_str() {
                "enable" => enable_dev(pool, req.origin.trim()).await,
                "add_origin" => add_origin(pool, req.origin.trim()).await,
                "disable" => disable_dev(pool).await,
                _ => Err((
                    400,
                    "invalid action: expected enable|add_origin|disable".into(),
                )),
            }
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

async fn toggle_status(pool: &DbPool, key: &str) -> Result<String, (u16, String)> {
    match get_setting(pool, key).await.map_err(db_err)? {
        Some(v) if v == "true" => Ok("enabled".into()),
        _ => Ok("disabled".into()),
    }
}

fn validate_origin(origin: &str) -> Result<(), (u16, String)> {
    if origin.is_empty() {
        return Err((400, "origin is required".into()));
    }
    if origin == "*" {
        return Ok(());
    }
    if !origin.starts_with("http://") && !origin.starts_with("https://") {
        return Err((
            400,
            format!("invalid origin {origin}: must start with http:// or https://"),
        ));
    }
    Ok(())
}

async fn enable_dev(pool: &DbPool, origin: &str) -> AdminResult {
    validate_origin(origin)?;
    set_setting(pool, settings_keys::WEBIMAP_ENABLED, "true")
        .await
        .map_err(db_err)?;
    set_setting(pool, settings_keys::WEBSMTP_ENABLED, "true")
        .await
        .map_err(db_err)?;
    let merged = merge_origin(pool, origin).await?;
    Ok((
        200,
        Some(json!({
            "status": "enabled",
            "webimap_enabled": "enabled",
            "websmtp_enabled": "enabled",
            "cors_origins": merged,
            "origin": origin,
        })),
    ))
}

async fn add_origin(pool: &DbPool, origin: &str) -> AdminResult {
    validate_origin(origin)?;
    let merged = merge_origin(pool, origin).await?;
    Ok((
        200,
        Some(json!({
            "status": "ok",
            "cors_origins": merged,
            "origin": origin,
        })),
    ))
}

async fn merge_origin(pool: &DbPool, origin: &str) -> Result<String, (u16, String)> {
    let existing = get_setting(pool, settings_keys::WEBMAIL_CORS_ORIGINS)
        .await
        .map_err(db_err)?
        .unwrap_or_default();
    let merged = append_origin(&existing, origin);
    set_setting(pool, settings_keys::WEBMAIL_CORS_ORIGINS, &merged)
        .await
        .map_err(db_err)?;
    Ok(merged)
}

async fn disable_dev(pool: &DbPool) -> AdminResult {
    set_setting(pool, settings_keys::WEBIMAP_ENABLED, "false")
        .await
        .map_err(db_err)?;
    set_setting(pool, settings_keys::WEBSMTP_ENABLED, "false")
        .await
        .map_err(db_err)?;
    Ok((
        200,
        Some(json!({
            "status": "disabled",
            "webimap_enabled": "disabled",
            "websmtp_enabled": "disabled",
        })),
    ))
}