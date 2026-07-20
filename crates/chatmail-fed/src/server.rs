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

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::post;
use axum::Router;
use chatmail_db::DbPool;
use chatmail_state::AppState;
use chatmail_types::Result;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::mxdeliv::{mxdeliv_handler, FedState};

pub fn federation_router(state: FedState) -> Router {
    // Axum defaults to 2 MiB; federated post-messages exceed that (cap: max_federation_size).
    let max_body = state.app.federation_size.effective().max(1) as usize;
    Router::new()
        .route("/mxdeliv", post(mxdeliv_handler))
        .layer(DefaultBodyLimit::max(max_body))
        .with_state(state)
}

#[allow(clippy::too_many_arguments)]
pub async fn run_http_listener(
    addr: &str,
    cancel: CancellationToken,
    tls: Option<Arc<ServerConfig>>,
    pool: DbPool,
    app: Arc<AppState>,
    primary_domain: String,
    local_domains: Vec<String>,
    extra: Option<Router>,
) -> Result<()> {
    let state = FedState {
        pool,
        app,
        primary_domain,
        local_domains,
    };
    let mut router = federation_router(state);
    if let Some(more) = extra {
        router = router.merge(more);
    }

    let listener = TcpListener::bind(addr).await?;
    let tls_acceptor = tls.map(TlsAcceptor::from);
    info!(%addr, tls = tls_acceptor.is_some(), "HTTP listener (federation + admin)");

    if tls_acceptor.is_none() {
        return axum::serve(listener, router)
            .with_graceful_shutdown(cancel.cancelled_owned())
            .await
            .map_err(|e| chatmail_types::ChatmailError::protocol(e.to_string()));
    }

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!(%addr, "HTTP listener stopped");
                break;
            }
            accept = listener.accept() => {
                let (stream, peer) = accept?;
                let app = router.clone();
                let acceptor = tls_acceptor.clone().expect("tls branch");
                tokio::spawn(async move {
                    let tls_stream = match acceptor.accept(stream).await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::debug!(%peer, error = %e, "HTTP TLS handshake failed");
                            return;
                        }
                    };
                    let io = TokioIo::new(tls_stream);
                    let hyper_svc = TowerToHyperService::new(app);
                    // WebSocket upgrades (WebIMAP /webimap/ws) require the upgrade-aware
                    // connection driver; plain serve_connection closes right after 101.
                    if let Err(e) = Builder::new(TokioExecutor::new())
                        .serve_connection_with_upgrades(io, hyper_svc)
                        .await
                    {
                        tracing::debug!(%peer, error = %e, "HTTP connection ended");
                    }
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chatmail_config::AppConfig;
    use chatmail_db::{init_memory_db, seed_install_defaults};
    use chatmail_state::AppState;
    use tower::ServiceExt;

    use super::*;
    use crate::mxdeliv::FedState;

    async fn test_router_with_federation_limit(limit: &str) -> Router {
        let pool = init_memory_db().await.unwrap();
        seed_install_defaults(&pool).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let app = Arc::new(AppState::with_quota_and_message_limit(
            dir.path(),
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &config,
            pool.clone(),
        ));
        app.hydrate(&pool, &config).await.unwrap();
        app.federation_size
            .set_limit(&pool, &config, limit)
            .await
            .unwrap();
        let state = FedState {
            pool,
            app,
            primary_domain: "example.org".into(),
            local_domains: chatmail_types::build_local_domains("example.org", None),
        };
        federation_router(state)
    }

    /// P7-UT04: Axum default body limit is 2 MiB; federation must accept larger POST bodies.
    #[tokio::test]
    async fn p7_ut04_federation_router_accepts_body_above_axum_default() {
        let router = test_router_with_federation_limit("4M").await;
        let body = vec![0u8; 3 * 1024 * 1024];
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mxdeliv")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    /// P7-UT05: bodies over `max_federation_size` are rejected at the HTTP layer (413).
    #[tokio::test]
    async fn p7_ut05_federation_router_rejects_body_over_limit() {
        let router = test_router_with_federation_limit("4M").await;
        let body = vec![0u8; 5 * 1024 * 1024];
        let resp = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mxdeliv")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
