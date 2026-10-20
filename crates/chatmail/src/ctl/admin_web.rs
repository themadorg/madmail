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

use chatmail_admin_web::{
    normalize_admin_web_path, resolve_admin_web_path, DEFAULT_ADMIN_WEB_PATH,
};
use chatmail_config::{AdminWebCommand, Args};
use chatmail_db::settings_keys::{ADMIN_WEB_ENABLED, ADMIN_WEB_PATH};
use chatmail_db::{delete_setting, set_setting};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;
use super::output::CtlOut;
use super::request_reload::notify_http_routes_changed;

/// CLI for the embedded admin-web SPA (`external/madmail-admin-web/build`), served at `admin_web_path`.
pub async fn admin_web(args: &Args, cmd: &AdminWebCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    ctx.require_db()?;

    match cmd {
        AdminWebCommand::Status => status(args, &ctx).await,
        AdminWebCommand::Enable => set_flag(args, &ctx, true).await,
        AdminWebCommand::Disable => set_flag(args, &ctx, false).await,
        AdminWebCommand::Path { path, reset } => {
            path_cmd(args, &ctx, path.as_deref(), *reset).await
        }
    }
}

async fn status(args: &Args, ctx: &CtlContext) -> Result<()> {
    let out = CtlOut::from_args(args, "admin-web status");
    let pool = ctx.open_pool().await?;
    let settings = ctx.load_settings_map().await?;
    let enabled = matches!(
        settings.get(ADMIN_WEB_ENABLED).map(|s| s.as_str()),
        Some("true")
    );

    let effective_path = resolve_admin_web_path(&ctx.config, &pool)
        .await
        .unwrap_or_else(|| DEFAULT_ADMIN_WEB_PATH.into());

    if out.is_json() {
        return out.emit(serde_json::json!({
            "enabled": enabled,
            "path": effective_path,
            "configuration_sources": pool.policy().sources(),
        }));
    }

    let db_path = settings
        .get(ADMIN_WEB_PATH)
        .filter(|s| !s.is_empty())
        .map(|s| s.as_str())
        .unwrap_or("(default)");

    out.blank();
    for (key, source) in pool.policy().sources() {
        if key == ADMIN_WEB_PATH || key == ADMIN_WEB_ENABLED {
            out.line(format!(
                "  Protected: {} ({source})",
                chatmail_config::settings_policy::setting_name(&key)
            ));
        }
    }
    out.line(format!(
        "  Admin Web Dashboard:  {}",
        if enabled { "enabled" } else { "disabled" }
    ));
    out.line(format!("  Admin Web Path:       {effective_path}"));
    if db_path != "(default)" && effective_path != db_path {
        out.line(format!("  DB override:          {db_path}"));
    } else if ctx.config.admin_web_path.is_some() && db_path == "(default)" {
        if let Some(ref p) = ctx.config.admin_web_path {
            out.line(format!("  Config path:          {p}"));
        }
    }
    out.line("  SPA source:           external/madmail-admin-web/build (embedded at compile time)");
    out.blank();
    Ok(())
}

async fn set_flag(args: &Args, ctx: &CtlContext, on: bool) -> Result<()> {
    let out = CtlOut::from_args(args, "admin-web");
    let pool = ctx.open_pool().await?;
    set_setting(&pool, ADMIN_WEB_ENABLED, if on { "true" } else { "false" }).await?;
    let path = resolve_admin_web_path(&ctx.config, &pool)
        .await
        .unwrap_or_else(|| DEFAULT_ADMIN_WEB_PATH.into());
    notify_http_routes_changed(ctx, &path).await?;
    out.done_msg(
        if on {
            format!("✅ Admin web dashboard enabled at {path}")
        } else {
            "🚫 Admin web dashboard disabled (returns 404 on the admin-web path)".into()
        },
        serde_json::json!({ "enabled": on, "path": path }),
        if on {
            format!("Admin web dashboard enabled at {path}")
        } else {
            "Admin web dashboard disabled".into()
        },
    )
}

async fn path_cmd(args: &Args, ctx: &CtlContext, path: Option<&str>, reset: bool) -> Result<()> {
    let out = CtlOut::from_args(args, "admin-web path");
    let pool = ctx.open_pool().await?;

    if reset {
        pool.check_settings_writable(&[ADMIN_WEB_PATH])?;
        let effective = ctx
            .config
            .admin_web_path
            .as_deref()
            .unwrap_or(DEFAULT_ADMIN_WEB_PATH);
        validate_web_path(&pool, &ctx.config, effective).await?;
        delete_setting(&pool, ADMIN_WEB_PATH).await?;
        let effective = resolve_admin_web_path(&ctx.config, &pool)
            .await
            .unwrap_or_else(|| DEFAULT_ADMIN_WEB_PATH.into());
        notify_http_routes_changed(ctx, &effective).await?;
        return out.done_msg(
            format!("🔄 Admin web path reset (effective: {effective})"),
            serde_json::json!({ "path": effective, "reset": true }),
            format!("Admin web path reset (effective: {effective})"),
        );
    }

    let Some(new_path) = path else {
        let effective = resolve_admin_web_path(&ctx.config, &pool)
            .await
            .unwrap_or_else(|| DEFAULT_ADMIN_WEB_PATH.into());
        if out.is_json() {
            return out.emit(serde_json::json!({ "path": effective }));
        }
        out.line(format!("Current admin web path: {effective}"));
        return Ok(());
    };

    let enable_needed = !chatmail_db::get_bool_setting(&pool, ADMIN_WEB_ENABLED, false).await?;
    pool.check_settings_writable(if enable_needed {
        &[ADMIN_WEB_PATH, ADMIN_WEB_ENABLED]
    } else {
        &[ADMIN_WEB_PATH]
    })?;
    let normalized = validate_web_path(&pool, &ctx.config, new_path).await?;
    set_setting(&pool, ADMIN_WEB_PATH, &normalized).await?;
    if enable_needed {
        set_setting(&pool, ADMIN_WEB_ENABLED, "true").await?;
    }
    notify_http_routes_changed(ctx, &normalized).await?;
    out.done_msg(
        format!("✅ Admin web dashboard enabled at {normalized}"),
        serde_json::json!({ "enabled": true, "path": normalized }),
        format!("Admin web dashboard enabled at {normalized}"),
    )
}

/// Validate against the effective API path before changing either setting.
async fn validate_web_path(
    pool: &chatmail_db::DbPool,
    config: &chatmail_config::AppConfig,
    path: &str,
) -> Result<String> {
    let path = chatmail_config::settings_policy::validate("admin_web_path", path)?;
    let path = normalize_admin_web_path(&path)
        .ok_or_else(|| ChatmailError::config("admin web path must not be empty"))?;
    let api = chatmail_db::get_setting(pool, chatmail_db::settings_keys::ADMIN_PATH)
        .await?
        .or_else(|| config.admin_path.clone())
        .unwrap_or_else(|| "/api/admin".into());
    let api = chatmail_config::settings_policy::validate("admin_path", &api)?;
    if path == api || path.starts_with(&format!("{api}/")) || api.starts_with(&format!("{path}/")) {
        return Err(ChatmailError::config(
            "admin API and web paths must not overlap",
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[tokio::test]
    async fn overlapping_set_and_reset_leave_settings_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let args = Args::try_parse_from([
            "chatmail",
            "--state-dir",
            dir.path().to_str().unwrap(),
            "--config",
            dir.path().join("missing.conf").to_str().unwrap(),
        ])
        .unwrap();
        let ctx = CtlContext::from_args(&args).unwrap();
        let pool = ctx.open_pool().await.unwrap();
        for path in ["/api/admin", "/api/admin/child", "/api"] {
            assert!(path_cmd(&args, &ctx, Some(path), false).await.is_err());
            assert!(chatmail_db::get_setting(&pool, ADMIN_WEB_PATH)
                .await
                .unwrap()
                .is_none());
            assert!(chatmail_db::get_setting(&pool, ADMIN_WEB_ENABLED)
                .await
                .unwrap()
                .is_none());
        }
        set_setting(&pool, ADMIN_WEB_PATH, "/panel").await.unwrap();
        set_setting(&pool, chatmail_db::settings_keys::ADMIN_PATH, "/admin/api")
            .await
            .unwrap();
        assert!(path_cmd(&args, &ctx, None, true).await.is_err());
        assert_eq!(
            chatmail_db::get_setting(&pool, ADMIN_WEB_PATH)
                .await
                .unwrap()
                .as_deref(),
            Some("/panel")
        );
    }
}
