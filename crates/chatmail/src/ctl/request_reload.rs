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

//! Ask a running server to soft-reload (POST `/admin/reload`).

use std::time::Duration;

use chatmail_state::ReloadScope;
use chatmail_types::{ChatmailError, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

use super::admin_url::{build_admin_url, build_site_base_url};
use super::context::CtlContext;
use crate::admin::resolve_admin_token;

#[derive(Debug, Deserialize)]
struct AdminEnvelope {
    status: u16,
    error: Option<String>,
}

/// Result of asking the running server to soft-reload HTTP routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoftReloadOutcome {
    /// Reload accepted; listeners will remount shortly.
    Accepted { api_url: String },
    /// No running server or admin API unreachable (e.g. wrong URL).
    ServerNotRunning,
}

/// POST `/admin/reload` on the running server.
pub async fn request_soft_reload(
    ctx: &CtlContext,
    url_override: Option<&str>,
    insecure: bool,
    scope: ReloadScope,
    wait: bool,
) -> Result<SoftReloadOutcome> {
    let token = match resolve_admin_token(&ctx.state_dir, &ctx.config) {
        Ok(t) => t,
        Err(_) => return Ok(SoftReloadOutcome::ServerNotRunning),
    };
    let settings = ctx.load_settings_map().await?;
    let api_url = url_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| {
            build_admin_url(&ctx.config, &settings)
                .trim_end_matches('/')
                .to_string()
        });

    let scope_str = match scope {
        ReloadScope::Full => "full",
        ReloadScope::HttpRoutes => "http",
    };

    let envelope = json!({
        "method": "POST",
        "resource": "/admin/reload",
        "headers": { "Authorization": format!("Bearer {token}") },
        "body": { "scope": scope_str, "wait": wait },
    });

    let client = build_http_client(insecure)?;
    let resp = match client
        .post(&api_url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "chatmail/ctl")
        .json(&envelope)
        .send()
    {
        Ok(r) => r,
        Err(e) if e.is_connect() || e.is_timeout() => {
            return Ok(SoftReloadOutcome::ServerNotRunning);
        }
        Err(e) => {
            return Err(ChatmailError::config(format!(
                "admin API request to {api_url}: {e}"
            )));
        }
    };

    if !resp.status().is_success() {
        return Ok(SoftReloadOutcome::ServerNotRunning);
    }

    let body_text = resp
        .text()
        .map_err(|e| ChatmailError::config(format!("read admin response: {e}")))?;
    let parsed: AdminEnvelope = match serde_json::from_str(&body_text) {
        Ok(p) => p,
        Err(_) => return Ok(SoftReloadOutcome::ServerNotRunning),
    };
    if let Some(err) = parsed.error.filter(|s| !s.is_empty()) {
        return Err(ChatmailError::config(format!("admin API: {err}")));
    }
    if parsed.status >= 400 {
        return Err(ChatmailError::config(format!(
            "admin API failed (status {})",
            parsed.status
        )));
    }
    Ok(SoftReloadOutcome::Accepted { api_url })
}

/// After changing admin-web settings, reload HTTP routes when the server is up.
pub async fn notify_http_routes_changed(ctx: &CtlContext, path: &str) -> Result<()> {
    const MAX_ATTEMPTS: usize = 8;
    let mut last_err = None;
    for attempt in 0..MAX_ATTEMPTS {
        match request_soft_reload(ctx, None, false, ReloadScope::HttpRoutes, true).await {
            Ok(SoftReloadOutcome::Accepted { api_url }) => {
                let site = {
                    let settings = ctx.load_settings_map().await?;
                    build_site_base_url(&ctx.config, &settings)
                };
                let web_url = format!(
                    "{}{}/",
                    site.trim_end_matches('/'),
                    path.trim_end_matches('/')
                );
                if wait_for_admin_web_path(ctx, path).await? {
                    println!("↻ Admin web live — open {web_url}");
                    println!("  API: {api_url}");
                } else {
                    println!("↻ HTTP routes reloaded — open {web_url} (verify in browser)");
                    println!("  API: {api_url}");
                }
                return Ok(());
            }
            Ok(SoftReloadOutcome::ServerNotRunning) if attempt + 1 < MAX_ATTEMPTS => {
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
            Ok(SoftReloadOutcome::ServerNotRunning) => {
                println!(
                    "ℹ  Server not running — changes apply on next `madmail run` / service start."
                );
                return Ok(());
            }
            Err(e) => {
                last_err = Some(e);
                if attempt + 1 < MAX_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                }
            }
        }
    }
    if let Some(e) = last_err {
        return Err(e);
    }
    println!("ℹ  Server not running — changes apply on next `madmail run` / service start.");
    Ok(())
}

async fn wait_for_admin_web_path(ctx: &CtlContext, path: &str) -> Result<bool> {
    let settings = ctx.load_settings_map().await?;
    let base = build_site_base_url(&ctx.config, &settings);
    let url = format!(
        "{}{}/",
        base.trim_end_matches('/'),
        path.trim_end_matches('/')
    );
    let client = build_http_client(false)?;
    let deadline = Duration::from_secs(15);
    let start = std::time::Instant::now();
    while start.elapsed() < deadline {
        if let Ok(resp) = client.get(&url).send() {
            if resp.status().is_success() {
                return Ok(true);
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Ok(false)
}

pub(crate) fn build_http_client(insecure: bool) -> Result<Client> {
    let mut builder = Client::builder().timeout(std::time::Duration::from_secs(120));
    if insecure {
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder
        .build()
        .map_err(|e| ChatmailError::config(format!("HTTP client: {e}")))
}
