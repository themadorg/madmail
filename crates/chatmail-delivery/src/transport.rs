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

use chatmail_state::TransportPolicy;
use chatmail_types::is_ipv4_literal;
use reqwest::Client;
use tracing::debug;
use tracing::warn;

use crate::federation_http::{federation_http_client, is_cleartext_redirect_blocked};
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

    // Transport policy blocks are permanent, like an MTA 5.7.x policy reject: the queue
    // drops the entry. Nothing was dialed here, so no per-transport stat is counted.
    let transports = ctx.state.federation_transport.effective(&domain);
    if !transports.any_allowed() {
        let reason = format!(
            "{} (recipient domain {domain})",
            chatmail_state::ALL_BLOCKED_REASON
        );
        warn!(rcpt = %job.rcpt_to, error = %reason, "federation: all transports blocked");
        return DeliveryOutcome::Permanent { reason };
    }

    ctx.state.federation_tracker.increment_queue(&domain);

    let signed = federated_rfc822(ctx, job).await;
    let job = OutboundJob {
        mail_from: job.mail_from.clone(),
        rcpt_to: job.rcpt_to.clone(),
        data: signed,
    };

    let target = resolve_federation_target(ctx, &domain).await;
    let client = federation_http_client(transports.http);
    // HELO/EHLO must identify *this* server, not the remote MX host.
    let helo = helo_name_for(ctx);

    let http_error = match &target {
        FederationTarget::MxdelivUrl(url) => {
            debug!(%url, rcpt = %job.rcpt_to, "federation: endpoint rewrite URL");
            match try_mxdeliv_url(client, url, &job, transports).await {
                Ok(method) => {
                    record_success(ctx, &domain, method);
                    return DeliveryOutcome::Success;
                }
                Err(e) => e,
            }
        }
        FederationTarget::Host(host) => {
            debug!(%host, rcpt = %job.rcpt_to, "federation: resolved host");
            match try_mxdeliv_host(client, host, &job, transports).await {
                Ok(method) => {
                    record_success(ctx, &domain, method);
                    return DeliveryOutcome::Success;
                }
                Err(e) => e,
            }
        }
    };

    if !transports.smtp {
        record_failure(ctx, &domain, http_error.scheme);
        let reason = format!(
            "{}; smtp blocked by outbound transport policy (recipient domain {domain})",
            http_error.reason
        );
        warn!(
            rcpt = %job.rcpt_to,
            permanent = http_error.permanent,
            error = %reason,
            "federation: SMTP blocked by transport policy"
        );
        return if http_error.permanent {
            DeliveryOutcome::Permanent { reason }
        } else {
            DeliveryOutcome::Temporary { reason }
        };
    }

    // Fall through to SMTP after HTTP failure: treating 4xx from a missing /mxdeliv
    // as permanent skipped SMTP entirely, which broke federation with any peer that
    // only speaks SMTP once HTTP 404'd.
    warn!(
        rcpt = %job.rcpt_to,
        permanent = http_error.permanent,
        error = %http_error.reason,
        "federation: HTTP /mxdeliv failed, trying SMTP fallback"
    );
    let http_reason = http_error.reason;

    let smtp_host = match &target {
        FederationTarget::Host(h) => h.clone(),
        FederationTarget::MxdelivUrl(url) => {
            host_from_mxdeliv_url(url).unwrap_or_else(|| domain.clone())
        }
    };
    match crate::federation_smtp::deliver(&smtp_host, &job, &helo).await {
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

/// Sign at send time so queued bytes stay unsigned and retries pick up a fresh `t=`.
async fn federated_rfc822(ctx: &DeliveryContext, job: &OutboundJob) -> Vec<u8> {
    match &ctx.dkim {
        Some(signer) => signer.sign_message(&job.data, &job.mail_from).await,
        None => job.data.clone(),
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
    /// Last `/mxdeliv` scheme tried or blocked (`"HTTPS"` / `"HTTP"`), for federation stats.
    scheme: &'static str,
}

const HTTPS_BLOCKED: &str = "https blocked by outbound transport policy";
const HTTP_BLOCKED: &str = "http blocked by outbound transport policy";

/// HTTPS then HTTP to `https://{host}/mxdeliv` / `http://{host}/mxdeliv`.
async fn try_mxdeliv_host(
    client: &Client,
    host: &str,
    job: &OutboundJob,
    policy: TransportPolicy,
) -> Result<&'static str, PostError> {
    if !policy.https && !policy.http {
        return Err(PostError {
            reason: format!("{HTTPS_BLOCKED}; {HTTP_BLOCKED}"),
            permanent: true,
            scheme: "HTTPS",
        });
    }

    let mut https_err: Option<PostError> = None;
    if policy.https {
        let https_url = format!("https://{host}/mxdeliv");
        match post_mxdeliv(client, &https_url, job).await {
            Ok(()) => return Ok("HTTPS"),
            Err(e) if e.permanent => return Err(e),
            Err(e) => {
                debug!(%https_url, error = %e.reason, "federation: HTTPS failed, trying HTTP");
                https_err = Some(e);
            }
        }
    }

    if !policy.http {
        let https_reason = https_err
            .map(|e| format!("https: {}", e.reason))
            .unwrap_or_else(|| HTTPS_BLOCKED.into());
        return Err(PostError {
            reason: format!("{https_reason}; {HTTP_BLOCKED}"),
            permanent: false,
            scheme: "HTTPS",
        });
    }

    let http_url = format!("http://{host}/mxdeliv");
    match post_mxdeliv(client, &http_url, job).await {
        Ok(()) => Ok("HTTP"),
        Err(e2) => Err(PostError {
            reason: match https_err {
                Some(e) => format!("https: {}; http: {}", e.reason, e2.reason),
                None => format!("{HTTPS_BLOCKED}; http: {}", e2.reason),
            },
            permanent: e2.permanent,
            scheme: "HTTP",
        }),
    }
}

/// POST to rewrite URL; if HTTPS fails transiently, retry as HTTP when allowed.
async fn try_mxdeliv_url(
    client: &Client,
    url: &str,
    job: &OutboundJob,
    policy: TransportPolicy,
) -> Result<&'static str, PostError> {
    let https = url.starts_with("https://");
    if https && !policy.https {
        if !policy.http {
            return Err(PostError {
                reason: format!("{HTTPS_BLOCKED}; {HTTP_BLOCKED}"),
                permanent: true,
                scheme: "HTTPS",
            });
        }
        let http_url = url.replacen("https://", "http://", 1);
        return match post_mxdeliv(client, &http_url, job).await {
            Ok(()) => Ok("HTTP"),
            Err(e) => Err(PostError {
                reason: format!("{HTTPS_BLOCKED}; http: {}", e.reason),
                permanent: e.permanent,
                scheme: "HTTP",
            }),
        };
    }
    if !https && !policy.http {
        return Err(PostError {
            reason: format!("{HTTP_BLOCKED} (endpoint rewrite {url})"),
            permanent: true,
            scheme: "HTTP",
        });
    }

    match post_mxdeliv(client, url, job).await {
        Ok(()) => Ok(scheme_label(url)),
        Err(e) if e.permanent => Err(e),
        Err(e) if https && policy.http => {
            let http_url = url.replacen("https://", "http://", 1);
            match post_mxdeliv(client, &http_url, job).await {
                Ok(()) => Ok("HTTP"),
                Err(e2) => Err(PostError {
                    reason: format!("https: {}; http: {}", e.reason, e2.reason),
                    permanent: e2.permanent,
                    scheme: "HTTP",
                }),
            }
        }
        Err(e) if https => Err(PostError {
            reason: format!("https: {}; {HTTP_BLOCKED}", e.reason),
            permanent: false,
            scheme: "HTTPS",
        }),
        Err(e) => Err(e),
    }
}

async fn post_mxdeliv(client: &Client, url: &str, job: &OutboundJob) -> Result<(), PostError> {
    let scheme = scheme_label(url);
    let res = client
        .post(url)
        .header("X-Mail-From", &job.mail_from)
        .header("X-Mail-To", &job.rcpt_to)
        .body(job.data.clone())
        .send()
        .await
        .map_err(|e| {
            // A peer redirecting the POST onto plain HTTP while `http` is blocked is a
            // policy block, so it is permanent like the other transport policy failures.
            if is_cleartext_redirect_blocked(&e) {
                PostError {
                    reason: format!("{HTTP_BLOCKED} (refused redirect from {url})"),
                    permanent: true,
                    scheme,
                }
            } else {
                PostError {
                    reason: e.to_string(),
                    permanent: false,
                    scheme,
                }
            }
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
            scheme,
        });
    }
    Err(PostError {
        reason: format!(
            "{} {}",
            res.status(),
            res.status().canonical_reason().unwrap_or("")
        ),
        permanent: false,
        scheme,
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

/// Host suitable for `https://HOST/mxdeliv` (bare IPv4, optional `:port`, bracketed IPv6, DNS).
///
/// Rules:
/// - `host:digits` where `host` has no `:` → keep as-is (IPv4 or DNS name + port)
/// - `[1.2.3.4]:port` → peel to `1.2.3.4:port`
/// - bare IPv4 → bare IPv4
/// - unbracketed IPv6 → `[ipv6]`
/// - `[ipv6]:port` / already-bracketed IPv6 → keep as-is
pub fn mxdeliv_host_for_url(host: &str) -> String {
    let host = host.trim();
    if host.is_empty() {
        return String::new();
    }

    // `[addr]:port` — peel brackets only for IPv4; keep IPv6 bracketed form.
    if host.starts_with('[') {
        if let Some(end) = host.find(']') {
            let inner = &host[1..end];
            let rest = &host[end + 1..];
            if let Some(port) = rest.strip_prefix(':') {
                if !port.is_empty()
                    && port.chars().all(|c| c.is_ascii_digit())
                    && is_ipv4_literal(inner)
                {
                    return format!("{inner}:{port}");
                }
            }
            // Bracketed IPv6 (± port) or bare bracketed IPv4 without port handling below.
            if is_ipv4_literal(inner) && rest.is_empty() {
                return inner.to_string();
            }
            return host.to_string();
        }
    }

    // Unbracketed `left:port` where left has no colon → host (IPv4 or DNS) + numeric port.
    if let Some((left, port)) = host.rsplit_once(':') {
        if !left.is_empty()
            && !left.contains(':')
            && !port.is_empty()
            && port.chars().all(|c| c.is_ascii_digit())
        {
            return format!("{left}:{port}");
        }
    }

    if is_ipv4_literal(host) {
        return host.to_string();
    }

    let bare = host.trim_matches(|c| c == '[' || c == ']');
    if is_ipv4_literal(bare) {
        return bare.to_string();
    }
    if bare.contains(':') {
        // Unbracketed IPv6 (no port form handled above only for single-colon host:port).
        if host.starts_with('[') {
            return host.to_string();
        }
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

    #[test]
    fn ipv4_with_port_not_wrapped_as_ipv6() {
        assert_eq!(mxdeliv_host_for_url("127.0.0.1:19080"), "127.0.0.1:19080");
        assert_eq!(mxdeliv_host_for_url("127.0.0.1"), "127.0.0.1");
        assert_eq!(mxdeliv_host_for_url("2001:db8::1"), "[2001:db8::1]");
    }

    #[test]
    fn host_port_forms_for_exchanger_and_dns() {
        assert_eq!(mxdeliv_host_for_url("localhost:19080"), "localhost:19080");
        assert_eq!(
            mxdeliv_host_for_url("relay.example:19080"),
            "relay.example:19080"
        );
        assert_eq!(mxdeliv_host_for_url("[127.0.0.1]:19080"), "127.0.0.1:19080");
        assert_eq!(
            mxdeliv_host_for_url("[2001:db8::1]:19080"),
            "[2001:db8::1]:19080"
        );
        assert_eq!(mxdeliv_host_for_url("[127.0.0.1]"), "127.0.0.1");
    }

    use wiremock::{Mock, MockServer, ResponseTemplate};

    const RCPT_DOMAIN: &str = "other.org";

    async fn test_ctx(policy: TransportPolicy) -> (DeliveryContext, tempfile::TempDir) {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let state = std::sync::Arc::new(chatmail_state::AppState::new(dir.path(), pool.clone()));
        state.federation_transport.set(policy);
        let ctx = DeliveryContext {
            pool,
            state,
            primary_domain: "example.org".into(),
            local_domains: vec!["example.org".into()],
            dkim: None,
        };
        (ctx, dir)
    }

    /// Deliver to `RCPT_DOMAIN` with `target` as its endpoint rewrite.
    async fn deliver_via(
        policy: TransportPolicy,
        target: &str,
    ) -> (DeliveryOutcome, DeliveryContext, tempfile::TempDir) {
        let (ctx, dir) = test_ctx(policy).await;
        chatmail_db::set_endpoint_override(&ctx.pool, RCPT_DOMAIN, target, "test")
            .await
            .unwrap();
        let outcome = deliver_remote(
            &ctx,
            &OutboundJob {
                mail_from: "a@example.org".into(),
                rcpt_to: format!("b@{RCPT_DOMAIN}"),
                data: b"test".to_vec(),
            },
        )
        .await;
        (outcome, ctx, dir)
    }

    fn stats(ctx: &DeliveryContext) -> chatmail_state::tracker::FederationStatRow {
        ctx.state
            .federation_tracker
            .snapshot()
            .into_iter()
            .find(|r| r.domain == RCPT_DOMAIN)
            .expect("federation stat row for recipient domain")
    }

    /// Plain-HTTP `/mxdeliv` peer. An `https://` URL to it fails at the TLS handshake,
    /// which is a transient error, so the code moves on to plain HTTP when allowed.
    async fn http_peer(status: u16) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::method("POST"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        server
    }

    async fn requests(server: &MockServer) -> usize {
        server.received_requests().await.unwrap_or_default().len()
    }

    const HTTPS_ONLY: TransportPolicy = TransportPolicy {
        https: true,
        http: false,
        smtp: false,
    };
    const NO_SMTP: TransportPolicy = TransportPolicy {
        https: true,
        http: true,
        smtp: false,
    };

    #[tokio::test]
    async fn http_blocked_host_target_never_posts_cleartext() {
        let peer = http_peer(200).await;
        let host = peer.address().to_string();
        let (outcome, ctx, _dir) = deliver_via(HTTPS_ONLY, &host).await;
        assert_eq!(requests(&peer).await, 0, "http:// must not be dialed");
        match outcome {
            DeliveryOutcome::Temporary { reason } => {
                assert!(reason.contains(HTTP_BLOCKED), "{reason}");
                assert!(
                    reason.contains("smtp blocked by outbound transport policy"),
                    "{reason}"
                );
                assert!(reason.contains(RCPT_DOMAIN), "{reason}");
            }
            other => panic!("expected temporary failure, got {other:?}"),
        }
        let row = stats(&ctx);
        assert_eq!(row.failed_https, 1);
        assert_eq!(row.queued_messages, 0);
    }

    #[tokio::test]
    async fn http_allowed_host_target_falls_back_to_cleartext() {
        let peer = http_peer(200).await;
        let host = peer.address().to_string();
        let (outcome, ctx, _dir) = deliver_via(NO_SMTP, &host).await;
        assert!(matches!(outcome, DeliveryOutcome::Success), "{outcome:?}");
        assert_eq!(requests(&peer).await, 1);
        assert_eq!(stats(&ctx).success_http, 1);
    }

    #[tokio::test]
    async fn http_blocked_rewrite_urls_never_post_cleartext() {
        let peer = http_peer(200).await;
        let addr = peer.address().to_string();

        let (outcome, ctx, _dir) = deliver_via(HTTPS_ONLY, &format!("http://{addr}/mxdeliv")).await;
        match outcome {
            DeliveryOutcome::Permanent { reason } => {
                assert!(reason.contains(HTTP_BLOCKED), "{reason}");
                assert!(reason.contains(RCPT_DOMAIN), "{reason}");
            }
            other => panic!("expected permanent policy failure, got {other:?}"),
        }
        assert_eq!(stats(&ctx).failed_http, 1);

        let (outcome, _ctx, _dir) =
            deliver_via(HTTPS_ONLY, &format!("https://{addr}/mxdeliv")).await;
        match outcome {
            DeliveryOutcome::Temporary { reason } => {
                assert!(reason.contains(HTTP_BLOCKED), "{reason}");
            }
            other => panic!("expected temporary failure, got {other:?}"),
        }
        assert_eq!(requests(&peer).await, 0, "http:// must not be dialed");
    }

    #[tokio::test]
    async fn smtp_blocked_records_http_failure() {
        let peer = http_peer(503).await;
        let url = format!("http://{}/mxdeliv", peer.address());
        let (outcome, ctx, _dir) = deliver_via(NO_SMTP, &url).await;
        match outcome {
            DeliveryOutcome::Temporary { reason } => {
                assert!(reason.contains("503"), "{reason}");
                assert!(
                    reason.contains("smtp blocked by outbound transport policy"),
                    "{reason}"
                );
            }
            other => panic!("expected temporary failure, got {other:?}"),
        }
        let row = stats(&ctx);
        assert_eq!(row.failed_http, 1);
        assert_eq!(row.queued_messages, 0);
        assert_eq!(requests(&peer).await, 1);
    }

    #[tokio::test]
    async fn smtp_blocked_client_error_is_permanent_policy_failure() {
        let peer = http_peer(404).await;
        let url = format!("http://{}/mxdeliv", peer.address());
        let (outcome, ctx, _dir) = deliver_via(NO_SMTP, &url).await;
        match outcome {
            DeliveryOutcome::Permanent { reason } => {
                assert!(reason.contains("404"), "{reason}");
                assert!(
                    reason.contains("smtp blocked by outbound transport policy"),
                    "{reason}"
                );
                assert!(reason.contains(RCPT_DOMAIN), "{reason}");
            }
            other => panic!("expected permanent failure, got {other:?}"),
        }
        assert_eq!(stats(&ctx).failed_http, 1);
    }

    #[tokio::test]
    async fn all_blocked_transports_fail_before_dialing() {
        let (ctx, _dir) = test_ctx(TransportPolicy {
            https: false,
            http: false,
            smtp: false,
        })
        .await;
        let outcome = deliver_remote(
            &ctx,
            &OutboundJob {
                mail_from: "a@example.org".into(),
                rcpt_to: "b@other.org".into(),
                data: b"test".to_vec(),
            },
        )
        .await;
        match outcome {
            DeliveryOutcome::Permanent { reason } => {
                assert!(
                    reason.starts_with(chatmail_state::ALL_BLOCKED_REASON),
                    "{reason}"
                );
                assert!(reason.contains("other.org"), "{reason}");
            }
            other => panic!("expected permanent policy failure, got {other:?}"),
        }
        assert!(ctx.state.federation_tracker.snapshot().is_empty());
    }

    #[tokio::test]
    async fn domain_override_blocks_before_dialing() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        chatmail_state::update_domain_transports(
            &pool,
            "Other.ORG",
            &[
                (chatmail_state::TransportName::Https, Some(false)),
                (chatmail_state::TransportName::Http, Some(false)),
                (chatmail_state::TransportName::Smtp, Some(false)),
            ],
        )
        .await
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let state = std::sync::Arc::new(chatmail_state::AppState::new(dir.path(), pool.clone()));
        state.federation_transport.hydrate(&pool).await.unwrap();
        assert!(state.federation_transport.get().any_allowed());
        let ctx = DeliveryContext {
            pool,
            state,
            primary_domain: "example.org".into(),
            local_domains: vec!["example.org".into()],
            dkim: None,
        };
        let outcome = deliver_remote(
            &ctx,
            &OutboundJob {
                mail_from: "a@example.org".into(),
                rcpt_to: "b@other.org".into(),
                data: b"test".to_vec(),
            },
        )
        .await;
        match outcome {
            DeliveryOutcome::Permanent { reason } => {
                assert_eq!(reason, chatmail_state::ALL_BLOCKED_REASON);
            }
            other => panic!("expected permanent policy failure, got {other:?}"),
        }
        assert!(ctx.state.federation_tracker.snapshot().is_empty());
    }
}
