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

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;

use chatmail_config::{AppConfig, DbMailPorts};
use chatmail_db::{settings_keys, DbPool};
use chatmail_types::Result;

use crate::allowed_ports::build_allowed_ports;
use crate::urls::ShadowsocksUrls;

/// Resolved Shadowsocks parameters (file config + admin DB overrides).
#[derive(Debug, Clone)]
pub struct ShadowsocksRuntime {
    pub listen_addr: String,
    pub password: String,
    pub cipher: String,
    pub mail_domain: String,
    pub public_ip: String,
    pub enabled: bool,
    pub ws_enabled: bool,
    pub grpc_enabled: bool,
    pub allowed_ports: HashSet<String>,
    pub tls_cert_path: PathBuf,
    pub tls_key_path: PathBuf,
}

impl ShadowsocksRuntime {
    pub fn configured(&self) -> bool {
        !self.listen_addr.is_empty() && !self.password.is_empty()
    }

    pub fn urls(&self, host_hint: &str) -> ShadowsocksUrls {
        ShadowsocksUrls::build(self, host_hint)
    }
}

/// Load runtime SS settings from `maddy.conf` and the settings DB.
pub async fn resolve_runtime(
    pool: &DbPool,
    file: &AppConfig,
    mail_domain: &str,
    state_dir: &std::path::Path,
) -> Result<ShadowsocksRuntime> {
    let db = chatmail_db::load_mail_port_overrides(pool).await?;
    resolve_runtime_with_db(pool, file, mail_domain, state_dir, &db).await
}

pub async fn resolve_runtime_with_db(
    pool: &DbPool,
    file: &AppConfig,
    mail_domain: &str,
    state_dir: &std::path::Path,
    db: &DbMailPorts,
) -> Result<ShadowsocksRuntime> {
    let settings = chatmail_db::get_settings_many(
        pool,
        &[
            settings_keys::SS_PASSWORD,
            settings_keys::SS_CIPHER,
            settings_keys::SS_PORT,
            settings_keys::SS_ENABLED,
        ],
    )
    .await?;
    resolve_runtime_from_settings(file, mail_domain, state_dir, db, &settings)
}

/// Same as [`resolve_runtime_with_db`] but uses a preloaded settings map (www cache).
pub fn resolve_runtime_from_settings(
    file: &AppConfig,
    mail_domain: &str,
    state_dir: &std::path::Path,
    db: &DbMailPorts,
    settings: &HashMap<String, String>,
) -> Result<ShadowsocksRuntime> {
    let ss_addr = file.ss_addr.clone().unwrap_or_default();
    let password = string_from_settings(
        settings,
        settings_keys::SS_PASSWORD,
        file.ss_password.as_deref().unwrap_or(""),
    );
    let cipher = string_from_settings(
        settings,
        settings_keys::SS_CIPHER,
        file.ss_cipher.as_deref().unwrap_or("aes-128-gcm"),
    );
    let listen_addr = listen_from_settings(&ss_addr, settings);
    let enabled =
        file.ss_configured() && bool_from_settings(settings, settings_keys::SS_ENABLED, true);
    // madmail-v2: raw TCP Shadowsocks only (no Xray WS/gRPC listeners).
    let ws_enabled = false;
    let grpc_enabled = false;

    let (tls_cert_path, tls_key_path) = resolve_tls_paths(file, state_dir);

    Ok(ShadowsocksRuntime {
        listen_addr,
        password,
        cipher,
        mail_domain: mail_domain.to_string(),
        public_ip: file.public_ip.clone().unwrap_or_default(),
        enabled,
        ws_enabled,
        grpc_enabled,
        allowed_ports: build_allowed_ports(file, db),
        tls_cert_path,
        tls_key_path,
    })
}

fn string_from_settings(map: &HashMap<String, String>, key: &str, default: &str) -> String {
    map.get(key)
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

fn bool_from_settings(map: &HashMap<String, String>, key: &str, default: bool) -> bool {
    map.get(key)
        .map(|v| {
            matches!(
                v.to_ascii_lowercase().as_str(),
                "true" | "1" | "yes" | "enabled"
            )
        })
        .unwrap_or(default)
}

fn listen_from_settings(ss_addr: &str, settings: &HashMap<String, String>) -> String {
    if ss_addr.is_empty() {
        return String::new();
    }
    let port_override = settings
        .get(settings_keys::SS_PORT)
        .filter(|p| !p.is_empty());
    let Some(port) = port_override else {
        return ss_addr.to_string();
    };
    replace_listen_port(ss_addr, port)
}

fn replace_listen_port(ss_addr: &str, port: &str) -> String {
    if let Ok(mut sa) = ss_addr.parse::<SocketAddr>() {
        if let Ok(p) = port.parse::<u16>() {
            sa.set_port(p);
            return sa.to_string();
        }
    }
    if let Some((host, _)) = ss_addr.rsplit_once(':') {
        return format!("{host}:{port}");
    }
    ss_addr.to_string()
}

fn resolve_tls_paths(file: &AppConfig, state_dir: &std::path::Path) -> (PathBuf, PathBuf) {
    let cert = file
        .ss_cert_path
        .clone()
        .or_else(|| file.tls_cert_path.clone())
        .unwrap_or_else(|| state_dir.join("certs/fullchain.pem"));
    let key = file
        .ss_key_path
        .clone()
        .or_else(|| file.tls_key_path.clone())
        .unwrap_or_else(|| state_dir.join("certs/privkey.pem"));
    (cert, key)
}

pub async fn ss_runtime_enabled(
    pool: &DbPool,
    file: &AppConfig,
    mail_domain: &str,
    state_dir: &std::path::Path,
) -> Result<bool> {
    let rt = resolve_runtime(pool, file, mail_domain, state_dir).await?;
    Ok(rt.configured() && rt.enabled)
}
