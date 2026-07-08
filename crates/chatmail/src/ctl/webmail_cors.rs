// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `madmail webmail-cors` — browser CORS origins for WebIMAP / WebSMTP / `/new`.

use chatmail_config::cli::WebmailCorsCommand;
use chatmail_config::Args;
use chatmail_db::{delete_setting, get_setting, set_setting, settings_keys, DbPool};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;
use super::output::CtlOut;

fn parse_origins_list(raw: &str) -> Vec<String> {
    raw.split(&[',', '\n', '\r'][..])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn join_origins(list: &[String]) -> String {
    list.join("\n")
}

fn append_origin(existing: &str, origin: &str) -> String {
    let mut list = parse_origins_list(existing);
    if !list.iter().any(|o| o == origin) {
        list.push(origin.to_string());
    }
    join_origins(&list)
}

fn remove_origin(existing: &str, origin: &str) -> String {
    let list: Vec<String> = parse_origins_list(existing)
        .into_iter()
        .filter(|o| o != origin)
        .collect();
    join_origins(&list)
}

pub fn validate_origin(origin: &str) -> Result<()> {
    let origin = origin.trim();
    if origin.is_empty() {
        return Err(ChatmailError::config("origin is required"));
    }
    if origin == "*" {
        return Ok(());
    }
    if !origin.starts_with("http://") && !origin.starts_with("https://") {
        return Err(ChatmailError::config(format!(
            "invalid origin {origin}: must start with http:// or https://"
        )));
    }
    Ok(())
}

fn validate_origins_value(value: &str) -> Result<()> {
    if value.len() > 4096 {
        return Err(ChatmailError::config(
            "cors origins list too long (max 4096)",
        ));
    }
    if value.trim() == "*" {
        return Ok(());
    }
    for origin in parse_origins_list(value) {
        validate_origin(&origin)?;
    }
    Ok(())
}

pub async fn webmail_cors(args: &Args, cmd: Option<&WebmailCorsCommand>) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        None | Some(WebmailCorsCommand::Status) => status(args, &pool).await,
        Some(WebmailCorsCommand::Set { value }) => set_value(args, &pool, value).await,
        Some(WebmailCorsCommand::Add { origin }) => add_origin(args, &pool, origin).await,
        Some(WebmailCorsCommand::Remove { origin }) => remove_one(args, &pool, origin).await,
        Some(WebmailCorsCommand::Reset) => reset(args, &pool).await,
        Some(WebmailCorsCommand::Enable { origin }) => enable_dev(args, &pool, origin).await,
    }
}

async fn current_origins(pool: &DbPool) -> Result<String> {
    Ok(get_setting(pool, settings_keys::WEBMAIL_CORS_ORIGINS)
        .await?
        .unwrap_or_default())
}

async fn status(args: &Args, pool: &DbPool) -> Result<()> {
    let out = CtlOut::from_args(args, "webmail-cors status");
    let raw = current_origins(pool).await?;
    let list = parse_origins_list(&raw);
    let webimap = service_on(pool, settings_keys::WEBIMAP_ENABLED).await?;
    let websmtp = service_on(pool, settings_keys::WEBSMTP_ENABLED).await?;

    if out.is_json() {
        return out.emit(serde_json::json!({
            "cors_origins": raw,
            "cors_origins_list": list,
            "webimap_enabled": webimap,
            "websmtp_enabled": websmtp,
        }));
    }

    out.blank();
    out.line(format!(
        "  WebIMAP:  {}",
        if webimap { "enabled" } else { "disabled" }
    ));
    out.line(format!(
        "  WebSMTP:  {}",
        if websmtp { "enabled" } else { "disabled" }
    ));
    out.line("  CORS origins (WebIMAP / WebSMTP / /new):");
    if list.is_empty() {
        out.line("    (none — browser cross-origin requests get no CORS headers)");
    } else {
        for o in &list {
            out.line(format!("    {o}"));
        }
    }
    out.line("  (effective on next HTTP request; no restart required)");
    out.blank();
    Ok(())
}

async fn service_on(pool: &DbPool, key: &str) -> Result<bool> {
    Ok(matches!(
        get_setting(pool, key).await?.as_deref(),
        Some("true")
    ))
}

async fn set_value(args: &Args, pool: &DbPool, value: &str) -> Result<()> {
    let out = CtlOut::from_args(args, "webmail-cors set");
    let value = value.trim();
    validate_origins_value(value)?;
    set_setting(pool, settings_keys::WEBMAIL_CORS_ORIGINS, value).await?;
    let list = parse_origins_list(value);
    out.done_msg(
        format!("🌐 Webmail CORS origins set ({})", list.len()),
        serde_json::json!({
            "cors_origins": value,
            "cors_origins_list": list,
        }),
        "Webmail CORS origins updated",
    )
}

async fn add_origin(args: &Args, pool: &DbPool, origin: &str) -> Result<()> {
    let out = CtlOut::from_args(args, "webmail-cors add");
    let origin = origin.trim();
    validate_origin(origin)?;
    let existing = current_origins(pool).await?;
    let merged = append_origin(&existing, origin);
    set_setting(pool, settings_keys::WEBMAIL_CORS_ORIGINS, &merged).await?;
    out.done_msg(
        format!("🌐 Added CORS origin {origin}"),
        serde_json::json!({
            "origin": origin,
            "cors_origins": merged,
            "cors_origins_list": parse_origins_list(&merged),
        }),
        format!("Added CORS origin {origin}"),
    )
}

async fn remove_one(args: &Args, pool: &DbPool, origin: &str) -> Result<()> {
    let out = CtlOut::from_args(args, "webmail-cors remove");
    let origin = origin.trim();
    if origin.is_empty() {
        return Err(ChatmailError::config("origin is required"));
    }
    let existing = current_origins(pool).await?;
    let merged = remove_origin(&existing, origin);
    if merged == existing {
        return Err(ChatmailError::config(format!("origin not in list: {origin}")));
    }
    if merged.is_empty() {
        delete_setting(pool, settings_keys::WEBMAIL_CORS_ORIGINS).await?;
    } else {
        set_setting(pool, settings_keys::WEBMAIL_CORS_ORIGINS, &merged).await?;
    }
    out.done_msg(
        format!("🌐 Removed CORS origin {origin}"),
        serde_json::json!({
            "origin": origin,
            "cors_origins": merged,
            "cors_origins_list": parse_origins_list(&merged),
        }),
        format!("Removed CORS origin {origin}"),
    )
}

async fn reset(args: &Args, pool: &DbPool) -> Result<()> {
    let out = CtlOut::from_args(args, "webmail-cors reset");
    delete_setting(pool, settings_keys::WEBMAIL_CORS_ORIGINS).await?;
    out.done_msg(
        "🔄 Webmail CORS origins cleared (effective immediately)",
        serde_json::json!({ "reset": true }),
        "Webmail CORS origins cleared",
    )
}

async fn enable_dev(args: &Args, pool: &DbPool, origin: &str) -> Result<()> {
    let out = CtlOut::from_args(args, "webmail-cors enable");
    let origin = origin.trim();
    validate_origin(origin)?;
    set_setting(pool, settings_keys::WEBIMAP_ENABLED, "true").await?;
    set_setting(pool, settings_keys::WEBSMTP_ENABLED, "true").await?;
    let existing = current_origins(pool).await?;
    let merged = append_origin(&existing, origin);
    set_setting(pool, settings_keys::WEBMAIL_CORS_ORIGINS, &merged).await?;
    out.done_msg(
        format!(
            "✅ Dev browser access enabled for {origin} (WebIMAP + WebSMTP + CORS)"
        ),
        serde_json::json!({
            "origin": origin,
            "webimap_enabled": true,
            "websmtp_enabled": true,
            "cors_origins": merged,
            "cors_origins_list": parse_origins_list(&merged),
        }),
        format!("Dev browser access enabled for {origin}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_remove_origin() {
        let a = append_origin("http://a:1", "http://b:2");
        assert!(a.contains("http://a:1"));
        assert!(a.contains("http://b:2"));
        let b = append_origin(&a, "http://a:1");
        assert_eq!(b.matches("http://a:1").count(), 1);
        let c = remove_origin(&b, "http://a:1");
        assert!(!c.contains("http://a:1"));
        assert!(c.contains("http://b:2"));
    }

    #[test]
    fn validate_origin_rejects_bad_scheme() {
        assert!(validate_origin("ftp://x").is_err());
        assert!(validate_origin("http://127.0.0.1:5173").is_ok());
        assert!(validate_origin("*").is_ok());
    }
}