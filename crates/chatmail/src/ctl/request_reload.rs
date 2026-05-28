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

use chatmail_types::{ChatmailError, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

use super::admin_url::build_admin_url;
use crate::admin::resolve_admin_token;
use super::context::CtlContext;

#[derive(Debug, Deserialize)]
struct AdminEnvelope {
    status: u16,
    error: Option<String>,
}

/// POST `/admin/reload` on the running server. Returns `Ok(true)` when accepted.
pub async fn try_request_soft_reload(ctx: &CtlContext) -> Result<bool> {
    let token = match resolve_admin_token(&ctx.state_dir, &ctx.config) {
        Ok(t) => t,
        Err(_) => return Ok(false),
    };
    let settings = ctx.load_settings_map().await?;
    let api_url = build_admin_url(&ctx.config, &settings).trim_end_matches('/').to_string();

    let envelope = json!({
        "method": "POST",
        "resource": "/admin/reload",
        "headers": { "Authorization": format!("Bearer {token}") },
        "body": {},
    });

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| ChatmailError::config(format!("HTTP client: {e}")))?;

    let resp = match client
        .post(&api_url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "chatmail/ctl")
        .json(&envelope)
        .send()
    {
        Ok(r) => r,
        Err(_) => return Ok(false),
    };

    if !resp.status().is_success() {
        return Ok(false);
    }

    let body_text = resp
        .text()
        .map_err(|e| ChatmailError::config(format!("read admin response: {e}")))?;
    let parsed: AdminEnvelope = match serde_json::from_str(&body_text) {
        Ok(p) => p,
        Err(_) => return Ok(false),
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
    Ok(true)
}

/// After changing admin-web settings, reload HTTP routes when the server is up.
pub async fn notify_http_routes_changed(ctx: &CtlContext) -> Result<()> {
    match try_request_soft_reload(ctx).await? {
        true => println!("↻ HTTP routes reloaded (admin-web changes are live)."),
        false => println!("ℹ  Server not running — changes apply on next `madmail run` / service start."),
    }
    Ok(())
}
