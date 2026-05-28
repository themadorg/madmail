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

//! Start/stop embedded iroh-relay and IMAP discovery (Madmail: `iroh_relay_url` + `IsIrohEnabled()`).

use std::net::{Ipv6Addr, SocketAddr};

use chatmail_config::AppConfig;
use chatmail_db::{get_bool_setting, get_setting, settings_keys, DbPool};
use chatmail_iroh::{spawn_iroh_relay, IrohDiscovery, IrohRelayHandle, IrohSpawnOpts};
use chatmail_types::Result;

/// Whether Iroh is active: static config **and** admin `__IROH_ENABLED__` (default on).
pub async fn iroh_runtime_enabled(pool: &DbPool, file_config: &AppConfig) -> Result<bool> {
    if !file_config.iroh_configured() {
        return Ok(false);
    }
    let admin_on = get_bool_setting(pool, settings_keys::IROH_ENABLED, true).await?;
    Ok(admin_on)
}

/// Build IMAP METADATA discovery (honours admin toggle + `__IROH_RELAY_URL__`).
pub async fn iroh_discovery(
    pool: &DbPool,
    file_config: &AppConfig,
    hostname: &str,
) -> Result<Option<IrohDiscovery>> {
    if !iroh_runtime_enabled(pool, file_config).await? {
        return Ok(None);
    }
    let url = effective_iroh_relay_url(pool, file_config, hostname).await?;
    Ok(IrohDiscovery::from_relay_url(url))
}

/// Spawn embedded iroh-relay according to config + admin toggle.
pub async fn start_iroh_relay(
    pool: &DbPool,
    file_config: &AppConfig,
    state_dir: &std::path::Path,
    hostname: &str,
) -> Result<Option<IrohRelayHandle>> {
    if !iroh_runtime_enabled(pool, file_config).await? {
        tracing::info!("Iroh relay disabled (config or admin toggle)");
        return Ok(None);
    }
    let listen = iroh_listen_addr(pool, file_config).await?;
    let opts = IrohSpawnOpts {
        listen,
        enable_stun: false,
    };
    let handle = spawn_iroh_relay(state_dir, opts)
        .await
        .map_err(|e| chatmail_types::ChatmailError::config(format!("iroh relay: {e:#}")))?;
    if let Some(url) = iroh_discovery(pool, file_config, hostname).await? {
        tracing::info!(
            listen = %handle.listen,
            relay_url = %url.relay_url,
            "Iroh relay started (WebXDC METADATA /shared/vendor/deltachat/irohrelay)"
        );
    }
    Ok(Some(handle))
}

async fn effective_iroh_relay_url(
    pool: &DbPool,
    file_config: &AppConfig,
    hostname: &str,
) -> Result<String> {
    if let Ok(Some(v)) = get_setting(pool, settings_keys::IROH_RELAY_URL).await {
        if !v.trim().is_empty() {
            return Ok(v);
        }
    }
    file_config
        .effective_iroh_relay_url(hostname)
        .ok_or_else(|| chatmail_types::ChatmailError::config("iroh_relay_url required"))
}

async fn effective_iroh_port(pool: &DbPool, file_config: &AppConfig) -> Result<u16> {
    if let Ok(Some(v)) = get_setting(pool, settings_keys::IROH_PORT).await {
        if let Ok(p) = v.trim().parse::<u16>() {
            if p != 0 {
                return Ok(p);
            }
        }
    }
    Ok(if file_config.iroh_port == 0 {
        3340
    } else {
        file_config.iroh_port
    })
}

async fn iroh_local_only(pool: &DbPool) -> Result<bool> {
    if let Ok(Some(v)) = get_setting(pool, settings_keys::IROH_LOCAL_ONLY).await {
        return Ok(v == "true");
    }
    Ok(false)
}

async fn iroh_listen_addr(pool: &DbPool, file_config: &AppConfig) -> Result<SocketAddr> {
    let port = effective_iroh_port(pool, file_config).await?;
    let ip = if iroh_local_only(pool).await? {
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
    } else {
        std::net::IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    };
    Ok(SocketAddr::new(ip, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::init_memory_db;

    fn iroh_file_config() -> AppConfig {
        AppConfig {
            iroh_enable: true,
            public_ip: Some("203.0.113.50".into()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn admin_iroh_disable_hides_discovery() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::set_setting(&pool, settings_keys::IROH_ENABLED, "false")
            .await
            .unwrap();
        let cfg = iroh_file_config();
        assert!(!iroh_runtime_enabled(&pool, &cfg).await.unwrap());
        assert!(iroh_discovery(&pool, &cfg, "mail.test").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn admin_iroh_enable_builds_url() {
        let pool = init_memory_db().await.unwrap();
        let cfg = iroh_file_config();
        let d = iroh_discovery(&pool, &cfg, "mail.test")
            .await
            .unwrap()
            .expect("discovery");
        assert_eq!(d.relay_url, "http://203.0.113.50:3340");
    }
}
