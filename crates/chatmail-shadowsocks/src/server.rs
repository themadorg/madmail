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

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use shadowsocks::config::{ServerAddr, ServerConfig, ServerType};
use shadowsocks::context::Context;
use shadowsocks::relay::socks5::Address;
use shadowsocks::relay::tcprelay::ProxyListener;
use tokio::io::copy_bidirectional;
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::cipher::parse_cipher;
use crate::runtime::ShadowsocksRuntime;

/// Running Shadowsocks listeners (TCP + optional Xray children).
pub struct ShadowsocksHandle {
    cancel: CancellationToken,
    tcp_join: tokio::task::JoinHandle<()>,
    xray: crate::xray::XrayChildren,
    enabled: Arc<AtomicBool>,
}

impl ShadowsocksHandle {
    pub fn shutdown(mut self) {
        self.cancel.cancel();
        self.xray.kill();
        self.tcp_join.abort();
    }

    /// Reflect admin `__SS_ENABLED__` (connections are dropped when false).
    pub fn set_accepting(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }
}

/// Start raw TCP Shadowsocks and optional Xray WS/gRPC transports.
pub async fn spawn_shadowsocks_server(
    rt: ShadowsocksRuntime,
) -> chatmail_types::Result<ShadowsocksHandle> {
    let method = parse_cipher(&rt.cipher).ok_or_else(|| {
        chatmail_types::ChatmailError::config(format!(
            "unsupported shadowsocks cipher: {}",
            rt.cipher
        ))
    })?;

    let listen: SocketAddr = rt.listen_addr.parse().map_err(|e| {
        chatmail_types::ChatmailError::config(format!("invalid ss_addr {}: {e}", rt.listen_addr))
    })?;

    let context = Context::new_shared(ServerType::Local);
    let svr_cfg = ServerConfig::new(ServerAddr::SocketAddr(listen), &rt.password, method)
        .map_err(|e| chatmail_types::ChatmailError::config(format!("shadowsocks config: {e}")))?;

    let listener = ProxyListener::bind(context.clone(), &svr_cfg)
        .await
        .map_err(|e| {
            chatmail_types::ChatmailError::config(format!(
                "shadowsocks listen on {}: {e}",
                rt.listen_addr
            ))
        })?;

    info!(
        listen = %rt.listen_addr,
        cipher = %rt.cipher,
        "Shadowsocks: raw TCP listener started"
    );

    let allowed: Arc<HashSet<String>> = Arc::new(rt.allowed_ports.clone());
    let cancel = CancellationToken::new();
    let child_cancel = cancel.child_token();
    let xray = crate::xray::spawn_xray_transports(&rt, rt.ws_enabled, rt.grpc_enabled)?;
    let enabled = Arc::new(AtomicBool::new(rt.enabled));

    let tcp_join = {
        let allowed = Arc::clone(&allowed);
        let enabled = Arc::clone(&enabled);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = child_cancel.cancelled() => break,
                    accept = listener.accept() => {
                        let Ok((mut stream, peer)) = accept else {
                            if child_cancel.is_cancelled() {
                                break;
                            }
                            continue;
                        };
                        if !enabled.load(Ordering::Relaxed) {
                            continue;
                        }
                        let allowed = Arc::clone(&allowed);
                        tokio::spawn(async move {
                            if let Err(e) = relay_connection(&mut stream, &allowed, peer).await {
                                if !matches!(e.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe) {
                                    warn!(%peer, error = %e, "shadowsocks relay");
                                }
                            }
                        });
                    }
                }
            }
        })
    };

    Ok(ShadowsocksHandle {
        cancel,
        tcp_join,
        xray,
        enabled,
    })
}

async fn relay_connection<S>(
    stream: &mut shadowsocks::relay::tcprelay::proxy_stream::server::ProxyServerStream<S>,
    allowed: &HashSet<String>,
    peer: SocketAddr,
) -> std::io::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let target = stream.handshake().await?;
    let port = target_port(&target);
    if !allowed.contains(&port) {
        warn!(%peer, %port, "shadowsocks: blocking port not used by this server");
        return Ok(());
    }
    // Always loopback — never forward to remote hosts or arbitrary ports (Madmail parity).
    let local = format!("127.0.0.1:{port}");
    debug!(%peer, ?target, %local, "shadowsocks: relaying to local service");
    let mut remote = TcpStream::connect(&local).await?;
    copy_bidirectional(stream, &mut remote).await?;
    Ok(())
}

fn target_port(addr: &Address) -> String {
    match addr {
        Address::SocketAddress(sa) => sa.port().to_string(),
        Address::DomainNameAddress(_, port) => port.to_string(),
    }
}

