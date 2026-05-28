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

use std::net::SocketAddr;
use std::path::Path;
use std::process::Stdio;

use serde_json::json;
use tokio::process::{Child, Command};
use tracing::{info, warn};

use crate::cipher::xray_method;
use crate::runtime::ShadowsocksRuntime;

pub struct XrayChildren {
    ws: Option<Child>,
    grpc: Option<Child>,
}

impl XrayChildren {
    pub fn kill(&mut self) {
        if let Some(mut c) = self.ws.take() {
            let _ = c.start_kill();
        }
        if let Some(mut c) = self.grpc.take() {
            let _ = c.start_kill();
        }
    }
}

pub fn spawn_xray_transports(
    rt: &ShadowsocksRuntime,
    ws_enabled: bool,
    grpc_enabled: bool,
) -> chatmail_types::Result<XrayChildren> {
    let xray = match which_xray() {
        Some(p) => p,
        None => {
            if ws_enabled || grpc_enabled {
                warn!("xray binary not found on PATH; WS/gRPC Shadowsocks transports disabled");
            }
            return Ok(XrayChildren {
                ws: None,
                grpc: None,
            });
        }
    };

    let listen: SocketAddr = rt.listen_addr.parse().map_err(|e| {
        chatmail_types::ChatmailError::config(format!("invalid ss_addr: {e}"))
    })?;
    let method = xray_method(&rt.cipher).ok_or_else(|| {
        chatmail_types::ChatmailError::config(format!(
            "unsupported cipher for xray: {}",
            rt.cipher
        ))
    })?;

    let host = listen.ip().to_string();
    let base_port = listen.port();
    let ws_port = base_port.saturating_add(2);
    let grpc_port = base_port.saturating_add(1);
    let allowed = allowed_ports_csv(&rt.allowed_ports);

    let mut ws = None;
    let mut grpc = None;

    if ws_enabled {
        let cfg = ws_config(&host, ws_port, method, &rt.password, &allowed);
        ws = spawn_xray(&xray, &cfg, "ws", ws_port)?;
    }
    if grpc_enabled {
        if !rt.tls_cert_path.is_file() || !rt.tls_key_path.is_file() {
            warn!(
                cert = %rt.tls_cert_path.display(),
                key = %rt.tls_key_path.display(),
                "shadowsocks gRPC: TLS cert/key missing; skipping gRPC transport"
            );
        } else {
            let server_name = if rt.public_ip.is_empty() {
                rt.mail_domain.trim_matches(|c| c == '[' || c == ']')
            } else {
                rt.public_ip.as_str()
            };
            let cfg = grpc_config(
                &host,
                grpc_port,
                method,
                &rt.password,
                &allowed,
                &rt.tls_cert_path,
                &rt.tls_key_path,
                server_name,
            );
            grpc = spawn_xray(&xray, &cfg, "grpc", grpc_port)?;
        }
    }

    Ok(XrayChildren { ws, grpc })
}

fn which_xray() -> Option<String> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join("xray");
            if candidate.is_file() {
                Some(candidate.to_string_lossy().into_owned())
            } else {
                None
            }
        })
    })
}

fn spawn_xray(
    xray_bin: &str,
    config: &serde_json::Value,
    label: &str,
    port: u16,
) -> chatmail_types::Result<Option<Child>> {
    let dir = std::env::temp_dir().join(format!(
        "chatmail-xray-{label}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| {
        chatmail_types::ChatmailError::config(format!("xray temp dir: {e}"))
    })?;
    let path = dir.join("config.json");
    std::fs::write(&path, serde_json::to_vec_pretty(config).unwrap()).map_err(|e| {
        chatmail_types::ChatmailError::config(format!("xray config write: {e}"))
    })?;

    let child = Command::new(xray_bin)
        .args(["run", "-c"])
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            chatmail_types::ChatmailError::config(format!("failed to spawn xray ({label}): {e}"))
        })?;

    info!(%port, transport = %label, "Shadowsocks: xray transport started");
    Ok(Some(child))
}

fn ws_config(
    listen_host: &str,
    port: u16,
    method: &str,
    password: &str,
    allowed_ports: &str,
) -> serde_json::Value {
    routing_config(
        listen_host,
        port,
        method,
        password,
        allowed_ports,
        json!({
            "network": "ws",
            "wsSettings": { "path": "/ss" }
        }),
        None,
    )
}

fn grpc_config(
    listen_host: &str,
    port: u16,
    method: &str,
    password: &str,
    allowed_ports: &str,
    cert: &Path,
    key: &Path,
    server_name: &str,
) -> serde_json::Value {
    routing_config(
        listen_host,
        port,
        method,
        password,
        allowed_ports,
        json!({
            "network": "grpc",
            "security": "tls",
            "grpcSettings": { "serviceName": "GunService" },
            "tlsSettings": {
                "serverName": server_name,
                "certificates": [{
                    "certificateFile": cert.display().to_string(),
                    "keyFile": key.display().to_string()
                }]
            }
        }),
        None,
    )
}

fn routing_config(
    listen_host: &str,
    port: u16,
    method: &str,
    password: &str,
    allowed_ports: &str,
    stream_settings: serde_json::Value,
    _extra: Option<()>,
) -> serde_json::Value {
    json!({
        "log": { "loglevel": "warning" },
        "inbounds": [{
            "listen": listen_host,
            "port": port,
            "protocol": "shadowsocks",
            "settings": {
                "method": method,
                "password": password,
                "network": "tcp"
            },
            "streamSettings": stream_settings
        }],
        "outbounds": [
            { "tag": "allow", "protocol": "freedom", "settings": {} },
            { "tag": "block", "protocol": "blackhole", "settings": {} }
        ],
        "routing": {
            "rules": [
                { "type": "field", "port": allowed_ports, "outboundTag": "allow" },
                { "type": "field", "port": "0-65535", "outboundTag": "block" }
            ]
        }
    })
}

fn allowed_ports_csv(ports: &std::collections::HashSet<String>) -> String {
    let mut v: Vec<_> = ports.iter().cloned().collect();
    v.sort();
    v.join(",")
}
