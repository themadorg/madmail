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

//! Shared HTTP client for federation (`/mxdeliv`) delivery.
//!
//! `reqwest::Client` must be reused across requests; building one per message wastes
//! memory (connection pools, TLS) and prevents connection reuse. There are two
//! clients: one with reqwest's default redirect policy, and one used while the
//! `http` transport is blocked that refuses redirects to anything but `https://`.

use std::fmt;
use std::sync::OnceLock;
use std::time::Duration;

use reqwest::redirect::Policy;
use reqwest::{Client, Url};

static FEDERATION_HTTP: OnceLock<Client> = OnceLock::new();
static FEDERATION_HTTPS_ONLY: OnceLock<Client> = OnceLock::new();

/// Redirect refused because it leaves `https://` while the `http` transport is blocked.
#[derive(Debug)]
pub struct CleartextRedirectBlocked;

impl fmt::Display for CleartextRedirectBlocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("redirect to http:// blocked by outbound transport policy (http)")
    }
}

impl std::error::Error for CleartextRedirectBlocked {}

/// Process-wide federation HTTP client (initialized on first use).
///
/// With `http_allowed == false` the client refuses any redirect whose target is not
/// `https://`, so a peer's 307/308 cannot move the POST onto plain HTTP.
pub fn federation_http_client(http_allowed: bool) -> &'static Client {
    if http_allowed {
        FEDERATION_HTTP.get_or_init(|| build_client(Policy::default()))
    } else {
        FEDERATION_HTTPS_ONLY.get_or_init(|| {
            build_client(Policy::custom(|attempt| {
                if redirect_leaves_https(attempt.url()) {
                    attempt.error(CleartextRedirectBlocked)
                } else {
                    // Keep reqwest's default hop limit.
                    Policy::default().redirect(attempt)
                }
            }))
        })
    }
}

/// `true` when a redirect target would leave TLS (anything but `https://`).
fn redirect_leaves_https(next: &Url) -> bool {
    next.scheme() != "https"
}

/// `true` when `err` came from a redirect refused by [`CleartextRedirectBlocked`].
pub fn is_cleartext_redirect_blocked(err: &reqwest::Error) -> bool {
    let mut source = std::error::Error::source(err);
    while let Some(e) = source {
        if e.is::<CleartextRedirectBlocked>() {
            return true;
        }
        source = e.source();
    }
    false
}

fn build_client(redirect: Policy) -> Client {
    Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(16)
        .redirect(redirect)
        .build()
        .expect("federation reqwest client")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn client_is_singleton() {
        for http_allowed in [true, false] {
            let a = federation_http_client(http_allowed) as *const Client;
            let b = federation_http_client(http_allowed) as *const Client;
            assert_eq!(a, b);
        }
        assert_ne!(
            federation_http_client(true) as *const Client,
            federation_http_client(false) as *const Client
        );
    }

    #[test]
    fn redirect_policy_only_keeps_https_targets() {
        let https = Url::parse("https://peer.example/mxdeliv").unwrap();
        let http = Url::parse("http://peer.example/mxdeliv").unwrap();
        assert!(!redirect_leaves_https(&https));
        assert!(redirect_leaves_https(&http));
    }

    /// The https-only client must refuse a 308 onto plain HTTP and never re-POST there.
    #[tokio::test]
    async fn https_only_client_refuses_cleartext_redirect() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mxdeliv"))
            .respond_with(
                ResponseTemplate::new(308)
                    .insert_header("Location", format!("{}/cleartext", server.uri())),
            )
            .mount(&server)
            .await;
        Mock::given(path("/cleartext"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let err = federation_http_client(false)
            .post(format!("{}/mxdeliv", server.uri()))
            .body(b"secret".to_vec())
            .send()
            .await
            .expect_err("redirect to http:// must be refused");
        assert!(err.is_redirect());
        assert!(is_cleartext_redirect_blocked(&err));
        server.verify().await;
    }

    #[tokio::test]
    async fn default_client_still_follows_redirects() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mxdeliv"))
            .respond_with(
                ResponseTemplate::new(308)
                    .insert_header("Location", format!("{}/next", server.uri())),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/next"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let res = federation_http_client(true)
            .post(format!("{}/mxdeliv", server.uri()))
            .body(b"ok".to_vec())
            .send()
            .await
            .unwrap();
        assert!(res.status().is_success());
        server.verify().await;
    }
}
