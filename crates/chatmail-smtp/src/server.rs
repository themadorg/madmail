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

use chatmail_db::DbPool;
use chatmail_state::AppState;
use chatmail_types::Result;
use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::session::{SmtpSession, SmtpSessionConfig};

pub async fn run_smtp_listener(
    addr: &str,
    cancel: CancellationToken,
    tls: Option<Arc<ServerConfig>>,
    starttls: Option<Arc<ServerConfig>>,
    ctx: Arc<AppState>,
    pool: DbPool,
    mut cfg: SmtpSessionConfig,
) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let implicit_tls = tls.is_some();
    let tls_acceptor = tls.map(TlsAcceptor::from);
    if !implicit_tls {
        cfg.starttls_config = starttls;
    }
    info!(
        %addr,
        tls = tls_acceptor.is_some(),
        starttls = cfg.starttls_config.is_some(),
        submission = cfg.require_auth,
        "SMTP listening"
    );
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!(%addr, "SMTP listener stopped");
                break;
            }
            accept = listener.accept() => {
                let (stream, peer) = accept?;
                // Disable Nagle so small SMTP command replies aren't delayed by 40–200ms.
                if let Err(e) = stream.set_nodelay(true) {
                    tracing::debug!(%peer, error = %e, "SMTP set_nodelay failed");
                }
                let ctx = Arc::clone(&ctx);
                let pool = pool.clone();
                let cfg = cfg.clone();
                let acceptor = tls_acceptor.clone();
                tokio::spawn(async move {
                    let mut session = SmtpSession::new(ctx, pool, cfg);
                    let result = if let Some(acceptor) = acceptor {
                        match acceptor.accept(stream).await {
                            Ok(tls_stream) => session.handle_tls_connection(tls_stream).await,
                            Err(e) => {
                                tracing::debug!(%peer, error = %e, "SMTP TLS handshake failed");
                                Ok(())
                            }
                        }
                    } else {
                        session.handle_connection(stream).await
                    };
                    if let Err(e) = result {
                        tracing::debug!(%peer, error = %e, "SMTP session ended");
                    }
                });
            }
        }
    }
    Ok(())
}
