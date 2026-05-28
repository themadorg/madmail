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

//! `chatmail reload` — Madmail `ctl/reload_config.go` (POST `/admin/reload`).

use chatmail_config::Args;
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

pub async fn reload(args: &Args, url_override: Option<&str>, insecure: bool) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    ctx.require_db()?;

    let token = resolve_admin_token(&ctx.state_dir, &ctx.config)?;
    let settings = ctx.load_settings_map().await?;

    let api_url = url_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| build_admin_url(&ctx.config, &settings).trim_end_matches('/').to_string());

    let envelope = json!({
        "method": "POST",
        "resource": "/admin/reload",
        "headers": { "Authorization": format!("Bearer {token}") },
        "body": {},
    });

    let client = build_http_client(insecure)?;
    let resp = client
        .post(&api_url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "chatmail/reload")
        .json(&envelope)
        .send()
        .map_err(|e| ChatmailError::config(format!("admin API request to {api_url}: {e}")))?;

    let status_code = resp.status();
    let body_text = resp
        .text()
        .map_err(|e| ChatmailError::config(format!("read admin response: {e}")))?;

    let parsed: AdminEnvelope = serde_json::from_str(&body_text).map_err(|e| {
        let preview = truncate(&body_text, 200);
        ChatmailError::config(format!(
            "invalid JSON from admin API (HTTP {status_code}): {e}; body: {preview}"
        ))
    })?;

    if let Some(err) = parsed.error.filter(|s| !s.is_empty()) {
        return Err(ChatmailError::config(format!("admin API: {err}")));
    }
    if parsed.status >= 400 {
        return Err(ChatmailError::config(format!(
            "admin API failed (status {})",
            parsed.status
        )));
    }
    if !status_code.is_success() {
        return Err(ChatmailError::config(format!(
            "HTTP {status_code} from admin API"
        )));
    }

    println!(
        "✅ Soft reload requested at {api_url} — listeners and HTTP routes restart in place (no process exit)."
    );
    Ok(())
}

fn build_http_client(insecure: bool) -> Result<Client> {
    let mut builder = Client::builder().timeout(std::time::Duration::from_secs(120));
    if insecure {
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder
        .build()
        .map_err(|e| ChatmailError::config(format!("HTTP client: {e}")))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
