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

use chatmail_admin_web::{resolve_admin_web_path, DEFAULT_ADMIN_WEB_PATH};
use chatmail_config::{AdminWebCommand, Args};
use chatmail_db::settings_keys::{ADMIN_WEB_ENABLED, ADMIN_WEB_PATH};
use chatmail_db::{delete_setting, set_setting};
use chatmail_types::Result;

use super::context::CtlContext;
use super::request_reload::notify_http_routes_changed;

/// CLI for the embedded admin-web SPA (`external/madmail-admin-web/build`), served at `admin_web_path`.
pub async fn admin_web(args: &Args, cmd: &AdminWebCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    ctx.require_db()?;

    match cmd {
        AdminWebCommand::Status => status(&ctx).await,
        AdminWebCommand::Enable => set_flag(&ctx, true).await,
        AdminWebCommand::Disable => set_flag(&ctx, false).await,
        AdminWebCommand::Path { path, reset } => path_cmd(&ctx, path.as_deref(), *reset).await,
    }
}

async fn status(ctx: &CtlContext) -> Result<()> {
    let pool = ctx.open_pool().await?;
    let settings = ctx.load_settings_map().await?;
    let enabled = match settings.get(ADMIN_WEB_ENABLED).map(|s| s.as_str()) {
        Some("true") => "enabled",
        _ => "disabled",
    };

    let effective_path = resolve_admin_web_path(&ctx.config, &pool)
        .await
        .unwrap_or_else(|| DEFAULT_ADMIN_WEB_PATH.into());

    let db_path = settings
        .get(ADMIN_WEB_PATH)
        .filter(|s| !s.is_empty())
        .map(|s| s.as_str())
        .unwrap_or("(default)");

    println!();
    println!("  Admin Web Dashboard:  {enabled}");
    println!("  Admin Web Path:       {effective_path}");
    if db_path != "(default)" && effective_path != db_path {
        println!("  DB override:          {db_path}");
    } else if ctx.config.admin_web_path.is_some() && db_path == "(default)" {
        if let Some(ref p) = ctx.config.admin_web_path {
            println!("  Config path:          {p}");
        }
    }
    println!("  SPA source:           external/madmail-admin-web/build (embedded at compile time)");
    println!();
    Ok(())
}

async fn set_flag(ctx: &CtlContext, on: bool) -> Result<()> {
    let pool = ctx.open_pool().await?;
    set_setting(
        &pool,
        ADMIN_WEB_ENABLED,
        if on { "true" } else { "false" },
    )
    .await?;
    if on {
        chatmail_admin_web::ensure_default_admin_web_path(&ctx.config, &pool).await?;
        let path = resolve_admin_web_path(&ctx.config, &pool)
            .await
            .unwrap_or_else(|| DEFAULT_ADMIN_WEB_PATH.into());
        println!("✅ Admin web dashboard enabled at {path}");
    } else {
        println!("🚫 Admin web dashboard disabled (returns 404 on the admin-web path)");
    }
    notify_http_routes_changed(ctx).await
}

async fn path_cmd(ctx: &CtlContext, path: Option<&str>, reset: bool) -> Result<()> {
    let pool = ctx.open_pool().await?;

    if reset {
        delete_setting(&pool, ADMIN_WEB_PATH).await?;
        let effective = resolve_admin_web_path(&ctx.config, &pool)
            .await
            .unwrap_or_else(|| DEFAULT_ADMIN_WEB_PATH.into());
        println!("🔄 Admin web path reset (effective: {effective})");
        return notify_http_routes_changed(ctx).await;
    }

    let Some(new_path) = path else {
        if let Some(effective) = resolve_admin_web_path(&ctx.config, &pool).await {
            println!("Current admin web path: {effective}");
        } else {
            println!("Current admin web path: {DEFAULT_ADMIN_WEB_PATH}");
        }
        return Ok(());
    };

    set_setting(&pool, ADMIN_WEB_PATH, new_path).await?;
    println!("✅ Admin web path set to {new_path}");
    notify_http_routes_changed(ctx).await
}
