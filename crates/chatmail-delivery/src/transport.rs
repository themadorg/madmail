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

use chatmail_types::is_ipv4_literal;
use reqwest::Client;
use tracing::debug;
use tracing::warn;

use crate::federation_http::federation_http_client;
use crate::router::{DeliveryContext, OutboundJob};

#[derive(Debug)]
pub enum DeliveryOutcome {
    Success,
    Temporary { reason: String },
    Permanent { reason: String },
}

/// Where to deliver a federated message (after `dns_overrides` / endpoint cache lookup).
#[derive(Debug, Clone, PartialEq, Eq)]
enum FederationTarget {
    /// Full pull URL from endpoint rewrite (scheme + host + optional path).
    MxdelivUrl(String),
    /// Hostname or IP — use `https://{host}/mxdeliv` then `http://{host}/mxdeliv`.
    Host(String),
}

pub async fn deliver_remote(ctx: &DeliveryContext, job: &OutboundJob) -> DeliveryOutcome {
    let domain = match job.rcpt_to.rsplit_once('@') {
        Some((_, d)) => d.to_string(),
        None => {
            return DeliveryOutcome::Permanent {
                reason: "bad rcpt address".into(),
            };
        }
    };

    // Outbound recipient-domain policy (ACCEPT blocklist / REJECT allowlist).
    let policy_mode = ctx.state.federation_policy.global_mode();
    if !ctx
        .state
        .federation_policy
        .check_policy(&domain, policy_mode)
    {
        return DeliveryOutcome::Permanent {
            reason: "Federation policy rejection".into(),
        };
    }

    ctx.state.federation_tracker.increment_queue(&domain);

    let target = resolve_federation_target(ctx, &domain).await;
    let client = federation_http_client();
    // HELO/EHLO must identify *this* server, not the remote MX host.
    let helo = helo_name_for(ctx);

    let http_reason = match &target {
        FederationTarget::MxdelivUrl(url) => {
            debug!(%url, rcpt = %job.rcpt_to, "federation: endpoint rewrite URL");
            match try_mxdeliv_url(client, url, job).await {
                Ok(method) => {
                    record_success(ctx, &domain, method);
                    return DeliveryOutcome::Success;
                }
                Err(e) => {
                    // Always fall through to SMTP after HTTP failure.
                    // Peers like nine.testrun.org often only accept real SMTP on :443;
                    // treating 4xx from missing /mxdeliv as permanent skipped SMTP entirely
                    // and broke WebSMTP/SMTP federation equally once HTTP returned 404.
                    warn!(
                        %url,
                        rcpt = %job.rcpt_to,
                        permanent = e.permanent,
                        error = %e.reason,
                        "federation: HTTP /mxdeliv failed, trying SMTP fallback"
                    );
                    e.reason
                }
            }
        }
        FederationTarget::Host(host) => {
            debug!(%host, rcpt = %job.rcpt_to, "federation: resolved host");
            match try_mxdeliv_host(client, host, job).await {
                Ok(method) => {
                    record_success(ctx, &domain, method);
                    return DeliveryOutcome::Success;
                }
                Err(e) => {
                    warn!(
                        %host,
                        rcpt = %job.rcpt_to,
                        permanent = e.permanent,
                        error = %e.reason,
                        "federation: HTTP /mxdeliv failed, trying SMTP fallback"
                    );
                    e.reason
                }
            }
        }
    };

    let smtp_host = match &target {
        FederationTarget::Host(h) => h.clone(),
        FederationTarget::MxdelivUrl(url) => {
            host_from_mxdeliv_url(url).unwrap_or_else(|| domain.clone())
        }
    };
    match crate::federation_smtp::deliver(&smtp_host, job, &helo).await {
        Ok(()) => {
            record_success(ctx, &domain, "SMTP");
            DeliveryOutcome::Success
        }
        Err(e) => {
            warn!(
                rcpt = %job.rcpt_to,
                host = %smtp_host,
                error = %e,
                "federation: SMTP fallback failed"
            );
            record_failure(ctx, &domain, "SMTP");
            DeliveryOutcome::Temporary {
                reason: format!("federation failed (http: {http_reason}; smtp: {e})"),
            }
        }
    }
}

fn helo_name_for(ctx: &DeliveryContext) -> String {
    let raw = ctx.primary_domain.trim();
    if raw.is_empty() {
        return "localhost".into();
    }
    // EHLO identity: bare IPv4, or bracketed form stripped for HELO text conventions.
    let bare = raw.trim_matches(|c| c == '[' || c == ']');
    if is_ipv4_literal(bare) {
        bare.to_string()
    } else {
        bare.to_ascii_lowercase()
    }
}

struct PostError {
    reason: String,
    permanent: bool,
}

/// HTTPS then HTTP to `https://{host}/mxdeliv` / `http://{host}/mxdeliv`.
async fn try_mxdeliv_host(
    client: &Client,
    host: &str,
    job: &OutboundJob,
) -> Result<&'static str, PostError> {
    let https_url = format!("https://{host}/mxdeliv");
    match post_mxdeliv(client, &https_url, job).await {
        Ok(()) => Ok("HTTPS"),
        Err(e) if e.permanent => Err(e),
        Err(e) => {
            debug!(%https_url, error = %e.reason, "federation: HTTPS failed, trying HTTP");
            let http_url = format!("http://{host}/mxdeliv");
            match post_mxdeliv(client, &http_url, job).await {
                Ok(()) => Ok("HTTP"),
                Err(e2) => Err(PostError {
                    reason: format!("https: {}; http: {}", e.reason, e2.reason),
                    permanent: e2.permanent,
                }),
            }
        }
    }
}

/// POST to rewrite URL; if HTTPS fails transiently, retry as HTTP.
async fn try_mxdeliv_url(
    client: &Client,
    url: &str,
    job: &OutboundJob,
) -> Result<&'static str, PostError> {
    match post_mxdeliv(client, url, job).await {
        Ok(()) => Ok(scheme_label(url)),
        Err(e) if e.permanent => Err(e),
        Err(e) if url.starts_with("https://") => {
            let http_url = url.replacen("https://", "http://", 1);
            match post_mxdeliv(client, &http_url, job).await {
                Ok(()) => Ok("HTTP"),
                Err(e2) => Err(PostError {
                    reason: format!("https: {}; http: {}", e.reason, e2.reason),
                    permanent: e2.permanent,
                }),
            }
        }
        Err(e) => Err(e),
    }
}

async fn post_mxdeliv(client: &Client, url: &str, job: &OutboundJob) -> Result<(), PostError> {
    let res = client
        .post(url)
        .header("X-Mail-From", &job.mail_from)
        .header("X-Mail-To", &job.rcpt_to)
        .body(job.data.clone())
        .send()
        .await
        .map_err(|e| PostError {
            reason: e.to_string(),
            permanent: false,
        })?;

    if res.status().is_success() {
        return Ok(());
    }
    if res.status().is_client_error() {
        return Err(PostError {
            reason: format!(
                "{} {}",
                res.status(),
                res.status().canonical_reason().unwrap_or("")
            ),
            permanent: true,
        });
    }
    Err(PostError {
        reason: format!(
            "{} {}",
            res.status(),
            res.status().canonical_reason().unwrap_or("")
        ),
        permanent: false,
    })
}

fn record_success(ctx: &DeliveryContext, domain: &str, method: &str) {
    ctx.state
        .federation_tracker
        .record_success(domain, 0, method);
    ctx.state.federation_tracker.decrement_queue(domain);
}

fn record_failure(ctx: &DeliveryContext, domain: &str, method: &str) {
    ctx.state.federation_tracker.record_failure(domain, method);
    ctx.state.federation_tracker.decrement_queue(domain);
}

/// Host suitable for `https://HOST/mxdeliv` (bare IPv4, bracketed IPv6, DNS names unchanged).
pub fn mxdeliv_host_for_url(host: &str) -> String {
    let bare = host.trim().trim_matches(|c| c == '[' || c == ']');
    if is_ipv4_literal(bare) {
        return bare.to_string();
    }
    if bare.contains(':') {
        return format!("[{bare}]");
    }
    bare.to_string()
}

fn normalize_rewrite_url(raw: &str) -> String {
    let mut raw = raw.trim().to_string();
    if !raw.contains("://") {
        raw = format!("https://{raw}");
    }
    let after_scheme = raw[raw.find("://").unwrap_or(0) + 3..].to_string();
    let slash_idx = after_scheme.find('/');
    if slash_idx.is_none() || after_scheme.get(slash_idx.unwrap()..) == Some("/") {
        raw = format!("{}/mxdeliv", raw.trim_end_matches('/'));
    }
    raw
}

fn host_from_mxdeliv_url(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let host_port = rest.split('/').next()?;
    let host = host_port
        .rsplit_once(':')
        .map(|(h, _)| h)
        .unwrap_or(host_port);
    Some(mxdeliv_host_for_url(host))
}

fn scheme_label(url: &str) -> &'static str {
    if url.starts_with("https://") {
        "HTTPS"
    } else {
        "HTTP"
    }
}

async fn resolve_federation_target(ctx: &DeliveryContext, domain: &str) -> FederationTarget {
    for key in lookup_keys(domain) {
        let row: Option<(String,)> = chatmail_db::db_fetch_optional!(
            &ctx.pool,
            (String,),
            "SELECT target_host FROM dns_overrides WHERE lookup_key = ?",
            key
        )
        .ok()
        .flatten();
        if let Some((h,)) = row {
            let h = h.trim();
            if !h.is_empty() {
                if h.contains("://") {
                    return FederationTarget::MxdelivUrl(normalize_rewrite_url(h));
                }
                return FederationTarget::Host(mxdeliv_host_for_url(h));
            }
        }
    }
    FederationTarget::Host(mxdeliv_host_for_url(domain))
}

/// Match Madmail endpoint_cache key forms (`1.1.1.1` vs `[1.1.1.1]`).
fn lookup_keys(domain: &str) -> Vec<String> {
    let lower = domain.to_ascii_lowercase();
    let stripped = lower.trim_matches(|c| c == '[' || c == ']');
    let mut keys = vec![lower.clone()];
    if stripped != lower {
        keys.push(stripped.to_string());
    }
    if !lower.starts_with('[') && stripped.contains('.') {
        keys.push(format!("[{stripped}]"));
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rewrite_appends_mxdeliv() {
        assert_eq!(normalize_rewrite_url("1.1.1.1"), "https://1.1.1.1/mxdeliv");
        assert_eq!(
            normalize_rewrite_url("https://relay.example.com"),
            "https://relay.example.com/mxdeliv"
        );
    }
}
