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

//! Embedded TURN server (webrtc-rs `turn` 0.11).

use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use tokio::net::UdpSocket;
use turn::auth::LongTermAuthHandler;
use turn::relay::relay_range::RelayAddressGeneratorRanges;
use turn::server::config::{ConnConfig, ServerConfig};
use turn::server::Server;
use webrtc_util::vnet::net::Net;

/// Options when starting embedded TURN.
#[derive(Debug, Clone)]
pub struct TurnSpawnOpts {
    /// Verbose logging (also enabled by `CHATMAIL_TURN_DEBUG=1`).
    pub debug: bool,
    /// Advertised via IMAP for Core `iceTransportPolicy: relay` (metadata only; webrtc turn
    /// still accepts STUN Binding on :3478).
    pub test_relay_only: bool,
    /// Inclusive UDP port range for relay allocations (default 49152–65535).
    pub relay_port_min: u16,
    pub relay_port_max: u16,
}

impl Default for TurnSpawnOpts {
    fn default() -> Self {
        Self {
            debug: turn_debug_from_env(),
            test_relay_only: turn_force_relay_test_from_env(),
            relay_port_min: DEFAULT_TURN_RELAY_PORT_MIN,
            relay_port_max: DEFAULT_TURN_RELAY_PORT_MAX,
        }
    }
}

/// Default minimum relay port ([RFC 8656] dynamic range).
pub const DEFAULT_TURN_RELAY_PORT_MIN: u16 = 49152;
/// Default maximum relay port (inclusive).
pub const DEFAULT_TURN_RELAY_PORT_MAX: u16 = 65535;

impl TurnSpawnOpts {
    /// Options for unit tests (no extra port-range tuning needed).
    pub fn for_tests() -> Self {
        Self {
            debug: false,
            test_relay_only: false,
            relay_port_min: DEFAULT_TURN_RELAY_PORT_MIN,
            relay_port_max: DEFAULT_TURN_RELAY_PORT_MAX,
        }
    }
}

/// `CHATMAIL_TURN_TEST_FORCE_RELAY=1` — advertise relay-only test metadata (see `turn-test.md`).
pub fn turn_force_relay_test_from_env() -> bool {
    std::env::var("CHATMAIL_TURN_TEST_FORCE_RELAY")
        .ok()
        .is_some_and(|v| env_flag_enabled(&v))
}

/// `CHATMAIL_TURN_DEBUG=1` / `true` / `yes` / `enable` / … enables verbose logging.
pub fn turn_debug_from_env() -> bool {
    std::env::var("CHATMAIL_TURN_DEBUG")
        .ok()
        .is_some_and(|v| env_flag_enabled(&v))
}

/// Flexible env flag: `1`, `true`, `yes`, `on`, `enable`, `enabled` (case-insensitive).
fn env_flag_enabled(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on" | "enable" | "enabled" | "t"
    )
}

/// Running TURN server (kept alive until dropped).
pub struct TurnServerHandle {
    _server: Arc<Server>,
    pub listen: SocketAddr,
    pub external: SocketAddr,
    pub realm: String,
}

/// Spawn TURN on `listen` (UDP), advertising relay address `external` to clients.
pub async fn spawn_turn_server(
    secret: &str,
    realm: &str,
    listen: SocketAddr,
    external: SocketAddr,
) -> Result<TurnServerHandle> {
    spawn_turn_server_with_opts(secret, realm, listen, external, TurnSpawnOpts::default()).await
}

pub async fn spawn_turn_server_with_opts(
    secret: &str,
    realm: &str,
    listen: SocketAddr,
    external: SocketAddr,
    opts: TurnSpawnOpts,
) -> Result<TurnServerHandle> {
    if opts.debug {
        tracing::info!(
            listen = %listen,
            external = %external,
            realm = %realm,
            test_relay_only = opts.test_relay_only,
            "TURN debug logging (webrtc turn)"
        );
    }

    let relay_ip = external.ip();
    let relay_range = (opts.relay_port_min, opts.relay_port_max);
    let conn_configs = build_conn_configs(listen, relay_ip, relay_range).await?;
    let n_ifaces = conn_configs.len();

    let auth_handler = Arc::new(LongTermAuthHandler::new(secret.to_string()));
    let server = Server::new(ServerConfig {
        conn_configs,
        realm: realm.to_string(),
        auth_handler,
        channel_bind_timeout: Duration::from_secs(0),
        alloc_close_notify: None,
    })
    .await
    .context("webrtc turn Server::new")?;

    let server = Arc::new(server);
    tracing::info!(
        listen = %listen,
        external = %external,
        realm = %realm,
        n_ifaces,
        "TURN server started (webrtc turn; open UDP {} and relay ports {}-{})",
        listen.port(),
        opts.relay_port_min,
        opts.relay_port_max
    );

    Ok(TurnServerHandle {
        _server: server,
        listen,
        external,
        realm: realm.to_string(),
    })
}

/// Bind addresses for TURN control (port 3478).
async fn build_conn_configs(
    listen: SocketAddr,
    relay_ip: IpAddr,
    relay_range: (u16, u16),
) -> Result<Vec<ConnConfig>> {
    let port = listen.port();
    let net = Arc::new(Net::new(None));
    let mut configs = Vec::new();

    let bind_ips: Vec<IpAddr> = if listen.ip().is_unspecified() {
        let mut ips: Vec<IpAddr> = listen_ips().into_iter().collect();
        if ips.is_empty() {
            ips.push(listen.ip());
        }
        ips
    } else {
        vec![listen.ip()]
    };

    for bind_ip in bind_ips {
        let conn = Arc::new(
            UdpSocket::bind((bind_ip, port))
                .await
                .with_context(|| format!("bind TURN UDP {bind_ip}:{port}"))?,
        );
        tracing::debug!(
            %bind_ip,
            %port,
            %relay_ip,
            relay_min = relay_range.0,
            relay_max = relay_range.1,
            "TURN listening"
        );
        configs.push(ConnConfig {
            conn,
            relay_addr_generator: Box::new(RelayAddressGeneratorRanges {
                relay_address: relay_ip,
                min_port: relay_range.0,
                max_port: relay_range.1,
                max_retries: 0,
                address: bind_ip.to_string(),
                net: net.clone(),
            }),
        });
    }

    Ok(configs)
}

fn listen_ips() -> BTreeSet<IpAddr> {
    let mut ip_set = BTreeSet::new();
    let interfaces = netdev::interface::get_interfaces();
    for interface in interfaces {
        for ip in interface.ip_addrs() {
            if !ip.is_loopback() && !is_link_local(ip) {
                ip_set.insert(ip);
            }
        }
    }
    ip_set
}

fn is_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => ipv4.is_link_local(),
        IpAddr::V6(ipv6) => ipv6.is_unicast_link_local(),
    }
}
