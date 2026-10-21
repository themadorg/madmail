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

//! Start/stop embedded TURN and IMAP discovery (Madmail: `enableTURN` + `IsTurnEnabled()`).

use std::net::SocketAddr;

use chatmail_config::{turn_relay_ports::TurnRelayPortRange, AppConfig};
use chatmail_db::{get_bool_setting, get_setting, settings_keys, DbPool};
use chatmail_turn::{
    spawn_turn_server_with_opts, turn_debug_from_env, turn_force_relay_test_from_env,
    TurnDiscovery, TurnServerHandle, TurnSpawnOpts,
};
use chatmail_types::Result;

/// Whether TURN is active: static `turn_enable` + secret **and** admin `__TURN_ENABLED__` (default on).
pub async fn turn_runtime_enabled(pool: &DbPool, file_config: &AppConfig) -> Result<bool> {
    if effective_turn_secret(pool, file_config)
        .await?
        .as_deref()
        .is_none_or(str::is_empty)
    {
        return Ok(false);
    }
    let admin_on =
        get_bool_setting(pool, settings_keys::TURN_ENABLED, file_config.turn_enable).await?;
    Ok(admin_on)
}

/// Build IMAP METADATA discovery (honours admin TURN toggle + DB overrides).
pub async fn turn_discovery(
    pool: &DbPool,
    file_config: &AppConfig,
    hostname: &str,
) -> Result<Option<TurnDiscovery>> {
    if !turn_runtime_enabled(pool, file_config).await? {
        return Ok(None);
    }
    let port = effective_turn_port(pool, file_config).await?;
    let secret = effective_turn_secret(pool, file_config).await?;
    let ttl = effective_turn_ttl(pool, file_config).await?;
    let turn_test_relay_only =
        file_config.turn_test_force_relay || turn_force_relay_test_from_env();
    Ok(TurnDiscovery::from_config(
        true,
        effective_turn_server(file_config, hostname),
        port,
        secret,
        ttl,
        turn_test_relay_only,
    ))
}

/// Spawn or stop embedded webrtc TURN according to config + admin toggle.
pub async fn start_turn_server(
    pool: &DbPool,
    file_config: &AppConfig,
    hostname: &str,
) -> Result<Option<TurnServerHandle>> {
    if !turn_runtime_enabled(pool, file_config).await? {
        tracing::info!("TURN relay disabled (config or admin toggle)");
        return Ok(None);
    }
    let secret = effective_turn_secret(pool, file_config)
        .await?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| chatmail_types::ChatmailError::config("turn_secret required"))?;
    let mut listen = turn_listen_addr(file_config)?;
    listen.set_port(effective_turn_port(pool, file_config).await?);
    if get_bool_setting(pool, settings_keys::TURN_LOCAL_ONLY, false).await? {
        listen.set_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    }
    let external = turn_external_addr(pool, file_config, listen, hostname).await?;
    let realm = effective_turn_realm(pool, file_config, hostname).await?;
    let turn_port = effective_turn_port(pool, file_config).await?;
    let relay_range = effective_turn_relay_port_range(pool, file_config).await?;
    relay_range
        .validate(turn_port)
        .map_err(chatmail_types::ChatmailError::config)?;

    warn_if_turn_listen_unreachable(listen, external);

    let force_relay_test = file_config.turn_test_force_relay || turn_force_relay_test_from_env();
    let opts = TurnSpawnOpts {
        debug: file_config.turn_debug || turn_debug_from_env(),
        test_relay_only: force_relay_test,
        relay_port_min: relay_range.min,
        relay_port_max: relay_range.max,
    };
    let handle = spawn_turn_server_with_opts(&secret, &realm, listen, external, opts)
        .await
        .map_err(|e| chatmail_types::ChatmailError::config(format!("turn server: {e:#}")))?;
    tracing::info!(
        listen = %handle.listen,
        external = %handle.external,
        realm = %handle.realm,
        turn_test_force_relay = force_relay_test,
        "TURN server started (open UDP {} and relay ports {}-{} on the relay IP)",
        listen.port(),
        relay_range.min,
        relay_range.max
    );
    if force_relay_test {
        tracing::info!(
            "turn_test_force_relay: IMAP turn-test-relay-only=1 (client relay policy); \
             STUN on :3478 still works — see turn-test.md"
        );
    }
    Ok(Some(handle))
}

/// Remote clients cannot reach a loopback TURN listener even if metadata advertises a public IP.
fn warn_if_turn_listen_unreachable(listen: SocketAddr, external: SocketAddr) {
    if listen.ip().is_loopback() && !external.ip().is_loopback() {
        tracing::warn!(
            listen = %listen,
            external = %external,
            "TURN listens on loopback but advertises a public relay address — \
             phones/desktops cannot connect unless you bind turn to 0.0.0.0 (e.g. turn udp://0.0.0.0:3478)"
        );
    }
    if external.ip().is_loopback() {
        tracing::warn!(
            external = %external,
            "TURN relay address is loopback — WebRTC relay candidates will not work for remote peers"
        );
    }
}

async fn effective_turn_port(pool: &DbPool, file_config: &AppConfig) -> Result<u16> {
    if let Ok(Some(v)) = get_setting(pool, settings_keys::TURN_PORT).await {
        if let Ok(p) = v.trim().parse::<u16>() {
            if p != 0 {
                return Ok(p);
            }
        }
    }
    Ok(if file_config.turn_port == 0 {
        3478
    } else {
        file_config.turn_port
    })
}

async fn effective_turn_secret(pool: &DbPool, file_config: &AppConfig) -> Result<Option<String>> {
    if let Ok(Some(v)) = get_setting(pool, settings_keys::TURN_SECRET).await {
        if !v.is_empty() {
            return Ok(Some(v));
        }
    }
    Ok(file_config.turn_secret.clone())
}

async fn effective_turn_realm(
    pool: &DbPool,
    file_config: &AppConfig,
    hostname: &str,
) -> Result<String> {
    if let Ok(Some(v)) = get_setting(pool, settings_keys::TURN_REALM).await {
        if !v.is_empty() {
            return Ok(v);
        }
    }
    Ok(file_config
        .turn_realm
        .clone()
        .unwrap_or_else(|| hostname.to_string()))
}

async fn effective_turn_relay_port_range(
    pool: &DbPool,
    file_config: &AppConfig,
) -> Result<TurnRelayPortRange> {
    let file = file_config.effective_turn_relay_port_range();
    let min =
        effective_turn_relay_port_bound(pool, settings_keys::TURN_RELAY_PORT_MIN, file.min).await?;
    let max =
        effective_turn_relay_port_bound(pool, settings_keys::TURN_RELAY_PORT_MAX, file.max).await?;
    Ok(TurnRelayPortRange::resolve(min, max))
}

async fn effective_turn_relay_port_bound(
    pool: &DbPool,
    key: &str,
    file_default: u16,
) -> Result<u16> {
    if let Ok(Some(v)) = get_setting(pool, key).await {
        if let Ok(p) = v.trim().parse::<u16>() {
            if p != 0 {
                return Ok(p);
            }
        }
    }
    Ok(file_default)
}

async fn effective_turn_ttl(pool: &DbPool, file_config: &AppConfig) -> Result<u64> {
    if let Ok(Some(v)) = get_setting(pool, settings_keys::TURN_TTL).await {
        if let Ok(t) = v.trim().parse::<u64>() {
            if t != 0 {
                return Ok(t);
            }
        }
    }
    Ok(if file_config.turn_ttl == 0 {
        86400
    } else {
        file_config.turn_ttl
    })
}

fn effective_turn_server(file_config: &AppConfig, hostname: &str) -> String {
    file_config.effective_turn_server(hostname)
}

/// Parse listen address for TURN (defaults to `0.0.0.0:3478` for remote clients).
fn turn_listen_addr(file_config: &AppConfig) -> Result<SocketAddr> {
    let raw = file_config
        .turn_listen_udp
        .as_deref()
        .or(file_config.turn_listen_tcp.as_deref())
        .unwrap_or("0.0.0.0:3478");
    let addr: SocketAddr = raw
        .parse()
        .map_err(|e| chatmail_types::ChatmailError::config(format!("turn listen: {e}")))?;
    Ok(addr)
}

/// External relay address advertised to clients.
async fn turn_external_addr(
    pool: &DbPool,
    file_config: &AppConfig,
    listen: SocketAddr,
    hostname: &str,
) -> Result<SocketAddr> {
    if let Ok(Some(ip)) = get_setting(pool, settings_keys::TURN_RELAY_IP).await {
        if let Ok(ip) = ip.trim().parse::<std::net::IpAddr>() {
            return Ok(SocketAddr::new(ip, listen.port()));
        }
    }
    if let Some(ip) = file_config.turn_relay_ip.as_deref() {
        if let Ok(ip) = ip.parse::<std::net::IpAddr>() {
            return Ok(SocketAddr::new(ip, listen.port()));
        }
    }
    if let Ok(ext) = effective_turn_server(file_config, hostname).parse::<std::net::IpAddr>() {
        return Ok(SocketAddr::new(ext, listen.port()));
    }
    Ok(listen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::init_memory_db;
    use std::net::{IpAddr, Ipv4Addr};

    fn turn_file_config() -> AppConfig {
        AppConfig {
            turn_enable: true,
            turn_secret: Some("s3cr3t".into()),
            turn_port: 3478,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn admin_turn_disable_hides_discovery() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::set_setting(&pool, settings_keys::TURN_ENABLED, "false")
            .await
            .unwrap();
        let cfg = turn_file_config();
        assert!(!turn_runtime_enabled(&pool, &cfg).await.unwrap());
        assert!(turn_discovery(&pool, &cfg, "mail.test")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn admin_turn_enable_with_file_config() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::set_setting(&pool, settings_keys::TURN_ENABLED, "true")
            .await
            .unwrap();
        let cfg = turn_file_config();
        let d = turn_discovery(&pool, &cfg, "mail.test")
            .await
            .unwrap()
            .expect("discovery");
        assert_eq!(d.port, 3478);
        assert_eq!(d.secret, "s3cr3t");
    }

    #[test]
    fn default_turn_listen_is_public() {
        let cfg = AppConfig::default();
        let listen = turn_listen_addr(&cfg).unwrap();
        assert_eq!(listen.ip(), IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(listen.port(), 3478);
    }

    #[tokio::test]
    async fn file_config_relay_port_range_used_when_db_unset() {
        let pool = init_memory_db().await.unwrap();
        let cfg = AppConfig {
            turn_enable: true,
            turn_secret: Some("s3cr3t".into()),
            turn_relay_port_min: 48_000,
            turn_relay_port_max: 48_100,
            ..Default::default()
        };
        let range = effective_turn_relay_port_range(&pool, &cfg).await.unwrap();
        assert_eq!(range.min, 48_000);
        assert_eq!(range.max, 48_100);
    }

    #[tokio::test]
    async fn start_turn_rejects_relay_range_overlapping_control_port() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::set_setting(&pool, settings_keys::TURN_ENABLED, "true")
            .await
            .unwrap();
        chatmail_db::set_setting(&pool, settings_keys::TURN_RELAY_PORT_MIN, "3000")
            .await
            .unwrap();
        chatmail_db::set_setting(&pool, settings_keys::TURN_RELAY_PORT_MAX, "4000")
            .await
            .unwrap();
        let cfg = turn_file_config();
        let err = match start_turn_server(&pool, &cfg, "mail.test").await {
            Ok(_) => panic!("expected relay range overlapping control port to be rejected"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("3478"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn admin_relay_port_range_override() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::set_setting(&pool, settings_keys::TURN_RELAY_PORT_MIN, "50000")
            .await
            .unwrap();
        chatmail_db::set_setting(&pool, settings_keys::TURN_RELAY_PORT_MAX, "50100")
            .await
            .unwrap();
        let cfg = turn_file_config();
        let range = effective_turn_relay_port_range(&pool, &cfg).await.unwrap();
        assert_eq!(range.min, 50000);
        assert_eq!(range.max, 50100);
    }

    #[test]
    fn warns_loopback_listen_with_public_external() {
        let listen: SocketAddr = "127.0.0.1:3478".parse().unwrap();
        let external: SocketAddr = "203.0.113.10:3478".parse().unwrap();
        warn_if_turn_listen_unreachable(listen, external);
    }
}
