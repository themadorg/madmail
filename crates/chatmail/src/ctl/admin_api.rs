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

use chatmail_config::{AdminApiCommand, Args};
use chatmail_db::settings_keys::{ADMIN_PATH, ADMIN_WEB_PATH};
use chatmail_db::{delete_setting, get_setting, set_setting};
use chatmail_types::{ChatmailError, Result};

use super::admin_url::build_admin_url;
use super::context::CtlContext;
use super::output::CtlOut;
use super::request_reload::{try_remount_candidates, RemountCandidates};
use crate::admin::resolve_admin_token;

const DEFAULT_ADMIN_PATH: &str = "/api/admin";

/// Show, set, or reset the admin API path and remount it on a running server.
pub async fn admin_api(args: &Args, cmd: &AdminApiCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    ctx.require_db()?;
    match cmd {
        AdminApiCommand::Path { path, reset } => {
            path_cmd(args, &ctx, path.as_deref(), *reset).await
        }
    }
}

async fn effective_path(ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<String> {
    let stored = get_setting(pool, ADMIN_PATH).await?;
    let raw = stored
        .filter(|v| !v.is_empty())
        .or_else(|| ctx.config.admin_path.clone())
        .unwrap_or_else(|| DEFAULT_ADMIN_PATH.into());
    chatmail_config::settings_policy::validate("admin_path", &raw)
        .map_err(|e| ChatmailError::config(e.to_string()))
}

async fn web_path(ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<String> {
    let stored = get_setting(pool, ADMIN_WEB_PATH).await?;
    let raw = stored
        .filter(|v| !v.is_empty())
        .or_else(|| ctx.config.admin_web_path.clone())
        .unwrap_or_else(|| "/admin".into());
    chatmail_config::settings_policy::validate("admin_web_path", &raw)
        .map_err(|e| ChatmailError::config(e.to_string()))
}

struct PathChange {
    previous_url: String,
    effective: String,
    new_url: String,
    reset: bool,
}

async fn apply_path(
    ctx: &CtlContext,
    pool: &chatmail_db::DbPool,
    path: Option<&str>,
    reset: bool,
) -> Result<PathChange> {
    let previous_url = {
        let settings = ctx.load_settings_map().await?;
        build_admin_url(&ctx.config, &settings)
    };

    if reset {
        pool.check_settings_writable(&[ADMIN_PATH])?;
        let fallback = ctx
            .config
            .admin_path
            .as_deref()
            .unwrap_or(DEFAULT_ADMIN_PATH);
        let fallback = chatmail_config::settings_policy::validate("admin_path", fallback)?;
        let web = web_path(ctx, pool).await?;
        if chatmail_config::settings_policy::admin_paths_overlap(&fallback, &web) {
            return Err(ChatmailError::config(
                "reset would overlap the admin API and web paths",
            ));
        }
        if chatmail_config::settings_policy::admin_path_uses_existing_post_route(&fallback) {
            return Err(ChatmailError::config(
                "admin API path conflicts with an existing POST route",
            ));
        }
        delete_setting(pool, ADMIN_PATH).await?;
        let effective = effective_path(ctx, pool).await?;
        let new_url = {
            let settings = ctx.load_settings_map().await?;
            build_admin_url(&ctx.config, &settings)
        };
        return Ok(PathChange {
            previous_url,
            effective,
            new_url,
            reset: true,
        });
    }

    let Some(new_path) = path else {
        let effective = effective_path(ctx, pool).await?;
        return Ok(PathChange {
            previous_url: previous_url.clone(),
            effective,
            new_url: previous_url,
            reset: false,
        });
    };

    pool.check_settings_writable(&[ADMIN_PATH])?;
    let normalized =
        chatmail_config::settings_policy::validate_admin_path_write("admin_path", new_path)?;
    let web = web_path(ctx, pool).await?;
    if chatmail_config::settings_policy::admin_paths_overlap(&normalized, &web) {
        return Err(ChatmailError::config(
            "admin API and web paths must not overlap",
        ));
    }
    if chatmail_config::settings_policy::admin_path_uses_existing_post_route(&normalized) {
        return Err(ChatmailError::config(
            "admin API path conflicts with an existing POST route",
        ));
    }
    set_setting(pool, ADMIN_PATH, &normalized).await?;
    let new_url = {
        let settings = ctx.load_settings_map().await?;
        build_admin_url(&ctx.config, &settings)
    };
    Ok(PathChange {
        previous_url,
        effective: normalized,
        new_url,
        reset: false,
    })
}

fn remount_candidates(previous_url: &str, default_url: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for url in [previous_url, default_url] {
        let url = url.trim().trim_end_matches('/').to_string();
        if !url.is_empty() && !urls.iter().any(|have: &String| have == &url) {
            urls.push(url);
        }
    }
    urls
}

async fn path_cmd(args: &Args, ctx: &CtlContext, path: Option<&str>, reset: bool) -> Result<()> {
    let out = CtlOut::from_args(args, "admin-api path");
    let pool = ctx.open_pool().await?;
    let default_url = {
        let mut settings = ctx.load_settings_map().await?;
        settings.remove(ADMIN_PATH);
        build_admin_url(&ctx.config, &settings)
    };
    let changed = apply_path(ctx, &pool, path, reset).await?;
    if path.is_none() && !reset {
        if out.is_json() {
            return out.emit(
                serde_json::json!({ "path": changed.effective, "api_url": changed.new_url }),
            );
        }
        out.line(format!("Current admin API path: {}", changed.effective));
        out.line(format!("  API: {}", changed.new_url));
        return Ok(());
    }

    // The saved path may not be the route that is mounted. Try the previous
    // URL, then the built-in URL. A refusal on one candidate does not abort
    // the command: the database change is already saved.
    let remount = match resolve_admin_token(&ctx.state_dir, &ctx.config) {
        Ok(token) => {
            try_remount_candidates(
                &token,
                &remount_candidates(&changed.previous_url, &default_url),
            )
            .await
        }
        Err(_) => RemountCandidates {
            applied: false,
            diagnostic: None,
        },
    };
    let applied = remount.applied;
    let message = if changed.reset {
        format!("Admin API path reset (effective: {})", changed.effective)
    } else {
        format!("Admin API path set to {}", changed.effective)
    };
    let mut data = serde_json::json!({
        "path": changed.effective,
        "api_url": changed.new_url,
        "reset": changed.reset,
        "applied": applied,
        "restart_required": !applied,
    });
    if let Some(diagnostic) = &remount.diagnostic {
        data["diagnostic"] = serde_json::Value::String(diagnostic.clone());
    }
    if applied {
        out.line(format!("✅ {message}"));
        out.line("↻ HTTP routes remounted");
    } else if changed.reset {
        out.line(format!("🔄 {message}"));
        out.line("ℹ  Saved. Restart madmail to apply it to a running server.");
    } else {
        out.line(format!("✅ {message}"));
        out.line("ℹ  Saved. Restart madmail to apply it to a running server.");
    }
    if let Some(diagnostic) = &remount.diagnostic {
        out.line(format!("ℹ  {diagnostic}"));
    }
    if out.is_json() {
        out.done_msg("", data, message)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[tokio::test]
    async fn set_reset_and_overlap_are_guarded() {
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

        apply_path(&ctx, &pool, Some("/api/secret"), false)
            .await
            .unwrap();
        assert_eq!(
            get_setting(&pool, ADMIN_PATH).await.unwrap().as_deref(),
            Some("/api/secret")
        );

        apply_path(&ctx, &pool, None, true).await.unwrap();
        assert!(get_setting(&pool, ADMIN_PATH).await.unwrap().is_none());

        set_setting(&pool, ADMIN_WEB_PATH, "/api/secret")
            .await
            .unwrap();
        assert!(apply_path(&ctx, &pool, Some("/api/secret"), false)
            .await
            .is_err());
        assert!(get_setting(&pool, ADMIN_PATH).await.unwrap().is_none());

        let conflict = match apply_path(&ctx, &pool, Some("/new"), false).await {
            Err(err) => err.to_string(),
            Ok(_) => panic!("/new must be rejected before it is saved"),
        };
        assert!(conflict.contains("existing POST route"), "{conflict}");
        assert!(get_setting(&pool, ADMIN_PATH).await.unwrap().is_none());
    }
}
