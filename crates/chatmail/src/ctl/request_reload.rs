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

    let timeout = std::time::Duration::from_secs(120);
    tokio::task::spawn_blocking(move || post_reload_blocking(&api_url, envelope, insecure, timeout))
        .await
        .map_err(|e| ChatmailError::config(format!("reload task failed: {e}")))?
}

/// Best-effort remount against several admin API URLs.
///
/// A candidate that refuses the reload, or cannot be reached, does not fail
/// the caller. The next URL is still tried. `applied` is true when one
/// candidate accepts. `diagnostic` explains the refusals when none do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemountCandidates {
    pub applied: bool,
    pub diagnostic: Option<String>,
}

/// Try each URL once, with a short timeout.
///
/// Expected request and API failures stay with that candidate. The database
/// change is already saved by the time this runs.
pub async fn try_remount_candidates(token: &str, urls: &[String]) -> RemountCandidates {
    let mut failures = Vec::new();
    for url in urls {
        let url = url.trim().trim_end_matches('/').to_string();
        if url.is_empty() {
            continue;
        }
        let envelope = json!({
            "method": "POST",
            "resource": "/admin/reload",
            "headers": { "Authorization": format!("Bearer {token}") },
            "body": { "scope": "http", "wait": true },
        });
        let timeout = Duration::from_secs(2);
        let posted_to = url.clone();
        match tokio::task::spawn_blocking(move || {
            post_reload_blocking(&posted_to, envelope, false, timeout)
        })
        .await
        {
            Ok(Ok(SoftReloadOutcome::Accepted { .. })) => {
                return RemountCandidates {
                    applied: true,
                    diagnostic: None,
                };
            }
            Ok(Ok(SoftReloadOutcome::ServerNotRunning)) => {}
            Ok(Err(err)) => failures.push(format!("{url}: {}", candidate_failure(&err))),
            Err(err) => failures.push(format!("{url}: reload task failed: {err}")),
        }
    }
    RemountCandidates {
        applied: false,
        diagnostic: if failures.is_empty() {
            None
        } else {
            Some(failures.join("; "))
        },
    }
}

fn candidate_failure(err: &ChatmailError) -> String {
    match err {
        ChatmailError::Config(msg) => msg.clone(),
        other => other.to_string(),
    }
}

fn post_reload_blocking(
    api_url: &str,
    envelope: serde_json::Value,
    insecure: bool,
    timeout: std::time::Duration,
) -> Result<SoftReloadOutcome> {
    // The admin token is inside the JSON body. A 307 or 308 would make the
    // client resend that body to the Location origin, so redirects are not followed.
    let mut builder = Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none());
    if insecure {
        builder = builder.danger_accept_invalid_certs(true);
    }
    let client = builder
        .build()
        .map_err(|e| ChatmailError::config(format!("HTTP client: {e}")))?;
    let resp = match client
        .post(api_url)
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
    if resp.status().is_redirection() {
        return Err(ChatmailError::config(format!(
            "admin API redirect from {api_url} was not followed"
        )));
    }
    if !resp.status().is_success() {
        return Ok(SoftReloadOutcome::ServerNotRunning);
    }
    let body_text = resp
        .text()
        .map_err(|e| ChatmailError::config(format!("read admin response: {e}")))?;
    drop(client);
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
    Ok(SoftReloadOutcome::Accepted {
        api_url: api_url.to_string(),
    })
}

/// After changing admin-web settings, reload HTTP routes when the server is up.
pub async fn notify_http_routes_changed(ctx: &CtlContext, path: &str) -> Result<()> {
    reload_http_routes(ctx, None, Some(path)).await
}

/// Remount HTTP routes without telling the operator to open a browser URL.
/// Used when the dashboard was just disabled.
pub async fn notify_http_routes_reloaded(ctx: &CtlContext) -> Result<()> {
    reload_http_routes(ctx, None, None).await
}

async fn reload_http_routes(
    ctx: &CtlContext,
    post_to: Option<&str>,
    open_path: Option<&str>,
) -> Result<()> {
    const MAX_ATTEMPTS: usize = 8;
    let mut last_err = None;
    for attempt in 0..MAX_ATTEMPTS {
        match request_soft_reload(ctx, post_to, false, ReloadScope::HttpRoutes, true).await {
            Ok(SoftReloadOutcome::Accepted { api_url }) => {
                if let Some(path) = open_path {
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
                }
                return Ok(());
            }
            Ok(SoftReloadOutcome::ServerNotRunning) if attempt + 1 < MAX_ATTEMPTS => {
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
            Ok(SoftReloadOutcome::ServerNotRunning) if open_path.is_none() => {
                return Ok(());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration as StdDuration;

    /// First candidate answers 307. The redirect target must receive nothing,
    /// and the next candidate is still asked.
    #[tokio::test]
    async fn a_reload_redirect_is_not_followed_and_the_next_candidate_is_tried() {
        let hits = Arc::new(AtomicUsize::new(0));
        let evil = TcpListener::bind("127.0.0.1:0").unwrap();
        let evil_addr = evil.local_addr().unwrap();
        evil.set_nonblocking(true).unwrap();
        let evil_hits = Arc::clone(&hits);
        let evil_thread = thread::spawn(move || {
            let start = std::time::Instant::now();
            while start.elapsed() < StdDuration::from_secs(3) {
                if evil.accept().is_ok() {
                    evil_hits.fetch_add(1, Ordering::Relaxed);
                }
                thread::sleep(StdDuration::from_millis(20));
            }
        });

        let first = serve_once(307, &format!("http://{evil_addr}/steal"));
        let second = serve_once(200, "");
        let urls = vec![first.url.clone(), second.url.clone()];
        let outcome = try_remount_candidates("token-value", &urls).await;
        assert!(outcome.applied, "{outcome:?}");
        assert_eq!(hits.load(Ordering::Relaxed), 0);
        assert_eq!(first.hits(), 1);
        assert_eq!(second.hits(), 1);
        drop(evil_thread);
    }

    struct OnceServer {
        url: String,
        hits: Arc<AtomicUsize>,
    }

    impl OnceServer {
        fn hits(&self) -> usize {
            self.hits.load(Ordering::Relaxed)
        }
    }

    fn serve_once(status: u16, location: &str) -> OnceServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let recorded = Arc::clone(&hits);
        let location = location.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.set_read_timeout(Some(StdDuration::from_secs(2)));
            let mut buf = [0_u8; 4096];
            let _ = stream.read(&mut buf);
            recorded.fetch_add(1, Ordering::Relaxed);
            let body = if status == 200 {
                r#"{"status":200}"#
            } else {
                ""
            };
            let location_header = if location.is_empty() {
                String::new()
            } else {
                format!("Location: {location}\r\n")
            };
            let response = format!(
                "HTTP/1.1 {status} OK\r\n{location_header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
        OnceServer {
            url: format!("http://127.0.0.1:{port}/api/admin"),
            hits,
        }
    }
}
