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

//! Minimal HTTP-01 challenge responder (port 80).

use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use hyper::header;
use hyper::service::Service;
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use parking_lot::RwLock;
use tokio::sync::oneshot;

#[derive(Clone, Debug)]
struct ChallengeEntry {
    host: String,
    key_authorization: String,
}

#[derive(Clone, Debug, Default)]
pub struct Http01Solver {
    challenges: Arc<RwLock<HashMap<String, ChallengeEntry>>>,
}

pub struct Http01Handle {
    tx: oneshot::Sender<()>,
    join: tokio::task::JoinHandle<hyper::Result<()>>,
}

impl Http01Solver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&self, address: &SocketAddr) -> hyper::Result<Http01Handle> {
        let (tx, rx) = oneshot::channel();
        let challenges = self.challenges.clone();
        let server = Server::try_bind(address)?.serve(MakeSvc(challenges)).with_graceful_shutdown(
            async move {
                let _ = rx.await;
            },
        );
        Ok(Http01Handle {
            tx,
            join: tokio::spawn(server),
        })
    }

    pub fn present(&self, host: String, token: String, key_authorization: String) {
        self.challenges.write().insert(
            token,
            ChallengeEntry {
                host,
                key_authorization,
            },
        );
    }
}

impl Http01Handle {
    pub async fn stop(self) -> Result<(), String> {
        let _ = self.tx.send(());
        self.join
            .await
            .map_err(|e| format!("HTTP-01 task join: {e}"))?
            .map_err(|e| format!("HTTP-01 server: {e}"))
    }
}

struct SolverService(Arc<RwLock<HashMap<String, ChallengeEntry>>>);

impl Service<Request<Body>> for SolverService {
    type Response = Response<Body>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        fn response(body: &'static str, status: StatusCode) -> Response<Body> {
            Response::builder()
                .status(status)
                .body(Body::from(body))
                .unwrap()
        }

        if req.method() != Method::GET {
            return Box::pin(async {
                Ok(response(
                    "method not allowed",
                    StatusCode::METHOD_NOT_ALLOWED,
                ))
            });
        }

        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .map(host_without_port);

        let token = req
            .uri()
            .path()
            .strip_prefix("/.well-known/acme-challenge/");

        if let Some(token) = token {
            let key_auth = {
                let challenges = self.0.read();
                challenges.get(token).and_then(|entry| {
                    let host_ok = host
                        .as_deref()
                        .map(|h| host_matches(&entry.host, h))
                        .unwrap_or(true);
                    if host_ok {
                        Some(entry.key_authorization.clone())
                    } else {
                        None
                    }
                })
            };
            if let Some(key_auth) = key_auth {
                return Box::pin(async move {
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/octet-stream")
                        .body(Body::from(key_auth))
                        .unwrap())
                });
            }
        }

        Box::pin(async { Ok(response("not found", StatusCode::NOT_FOUND)) })
    }
}

struct MakeSvc(Arc<RwLock<HashMap<String, ChallengeEntry>>>);

impl<T> Service<T> for MakeSvc {
    type Response = SolverService;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: T) -> Self::Future {
        let challenges = self.0.clone();
        Box::pin(async move { Ok(SolverService(challenges)) })
    }
}

fn host_without_port(host: &str) -> String {
    let host = host.trim();
    if let Some(end) = host.find(']') {
        let inner = &host[1..end];
        if let Ok(ip) = inner.parse::<std::net::IpAddr>() {
            return ip.to_string();
        }
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ip.to_string();
    }
    host.split(':').next().unwrap_or(host).to_string()
}

/// Compare ACME identifier host with the HTTP `Host` header (DNS or IP, with optional IPv6 brackets).
pub fn host_matches(expected: &str, actual: &str) -> bool {
    normalize_host(expected) == normalize_host(actual)
}

fn normalize_host(host: &str) -> String {
    let h = host_without_port(host);
    if let Ok(ip) = h.parse::<std::net::IpAddr>() {
        return ip.to_string();
    }
    h.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_matching_ipv4_and_brackets() {
        assert!(host_matches("1.1.1.1", "1.1.1.1"));
        assert!(host_matches("1.1.1.1", "1.1.1.1:80"));
    }

    #[test]
    fn host_matching_ipv6() {
        assert!(host_matches("2001:db8::1", "2001:db8::1"));
        assert!(host_matches("2001:db8::1", "[2001:db8::1]"));
    }

    #[test]
    fn host_matching_dns() {
        assert!(host_matches("Example.COM", "example.com"));
        assert!(!host_matches("a.example.com", "b.example.com"));
    }
}
