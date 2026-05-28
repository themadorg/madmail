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

use axum::routing::post;
use axum::Router;
use chatmail_state::AppState;
use chatmail_types::Result;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use rustls::ServerConfig;
use chatmail_db::DbPool;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tokio_rustls::TlsAcceptor;
use tracing::info;

use crate::mxdeliv::{mxdeliv_handler, FedState};

pub fn federation_router(state: FedState) -> Router {
    Router::new()
        .route("/mxdeliv", post(mxdeliv_handler))
        .with_state(state)
}

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
                    if let Err(e) = Builder::new(TokioExecutor::new())
                        .serve_connection(io, hyper_svc)
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
