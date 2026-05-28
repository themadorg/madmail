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

use base64::{engine::general_purpose::STANDARD, Engine as _};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

use crate::runtime::ShadowsocksRuntime;

/// Client URLs and v2rayNG JSON (Madmail `getShadowsocks*` / `getV2rayNGConfig*`).
#[derive(Debug, Clone, Default)]
pub struct ShadowsocksUrls {
    pub shadowsocks_url: String,
    pub ws_url: String,
    pub grpc_url: String,
    pub v2ray_ng_ws: String,
    pub v2ray_ng_grpc: String,
}

impl ShadowsocksUrls {
    pub fn build(rt: &ShadowsocksRuntime, host_hint: &str) -> Self {
        if !rt.configured() || !rt.enabled {
            return Self::default();
        }
        let host = resolve_host(rt, host_hint);
        let (base_port, ws_port, grpc_port) = effective_ports(rt);
        let auth = ss_auth_segment(&rt.cipher, &rt.password);

        let shadowsocks_url = format!(
            "ss://{auth}@{host}:{base_port}#{}",
            url_encode_fragment(&host)
        );

        let ws_url = if rt.ws_enabled {
            let plugin = utf8_percent_encode(
                &format!("v2ray-plugin;mode=websocket;host={host};path=/ss"),
                NON_ALPHANUMERIC,
            )
            .to_string();
            format!(
                "ss://{auth}@{host}:{ws_port}/?plugin={plugin}#{}",
                url_encode_fragment(&host)
            )
        } else {
            String::new()
        };

        let grpc_url = if rt.grpc_enabled {
            let plugin = utf8_percent_encode(
                &format!("v2ray-plugin;mode=grpc;host={host}"),
                NON_ALPHANUMERIC,
            )
            .to_string();
            format!(
                "ss://{auth}@{host}:{grpc_port}/?plugin={plugin}#{}",
                url_encode_fragment(&host)
            )
        } else {
            String::new()
        };

        let allowed = allowed_ports_csv(&rt.allowed_ports);
        let v2ray_ng_ws = if rt.ws_enabled {
            v2ray_ng_ws_json(&host, ws_port, &rt.cipher, &rt.password, &allowed)
        } else {
            String::new()
        };
        let v2ray_ng_grpc = if rt.grpc_enabled {
            v2ray_ng_grpc_json(&host, grpc_port, &rt.cipher, &rt.password, &allowed)
        } else {
            String::new()
        };

        Self {
            shadowsocks_url,
            ws_url,
            grpc_url,
            v2ray_ng_ws,
            v2ray_ng_grpc,
        }
    }
}

fn ss_auth_segment(cipher: &str, password: &str) -> String {
    let user_info = format!("{cipher}:{password}");
    let mut auth = STANDARD.encode(user_info.as_bytes());
    while auth.ends_with('=') {
        auth.pop();
    }
    auth
}

fn resolve_host(rt: &ShadowsocksRuntime, host_hint: &str) -> String {
    let (host, _) = split_host_port(&rt.listen_addr);
    let host = host.as_str().trim();
    if host.is_empty() || host == "0.0.0.0" || host == "::" || host == "[::]" {
        let hint = host_hint.trim();
        if hint.is_empty() {
            return rt.mail_domain.trim_matches(|c| c == '[' || c == ']').to_string();
        }
        let (h, _) = split_host_port(hint);
        if h.is_empty() { hint.to_string() } else { h }
    } else {
        host.trim_matches(|c| c == '[' || c == ']').to_string()
    }
}

fn effective_ports(rt: &ShadowsocksRuntime) -> (String, u16, u16) {
    let (_, base) = split_host_port(&rt.listen_addr);
    let base_port = if base.is_empty() {
        "8388".to_string()
    } else {
        base
    };
    let base_n: u16 = base_port.parse().unwrap_or(8388);
    (base_port, base_n + 2, base_n + 1)
}

fn split_host_port(addr: &str) -> (String, String) {
    if let Ok(sa) = addr.parse::<std::net::SocketAddr>() {
        return (sa.ip().to_string(), sa.port().to_string());
    }
    if let Some((h, p)) = addr.rsplit_once(':') {
        (h.to_string(), p.to_string())
    } else {
        (addr.to_string(), String::new())
    }
}

fn url_encode_fragment(s: &str) -> String {
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
}

fn allowed_ports_csv(ports: &std::collections::HashSet<String>) -> String {
    let mut v: Vec<_> = ports.iter().cloned().collect();
    v.sort();
    v.join(",")
}

fn v2ray_ng_ws_json(host: &str, port: u16, cipher: &str, password: &str, allowed: &str) -> String {
    format!(
        r#"{{
  "dns": {{"servers": ["1.1.1.1", "8.8.8.8"]}},
  "inbounds": [{{"listen": "127.0.0.1", "port": 10808, "protocol": "socks", "settings": {{"auth": "noauth", "udp": true}}, "sniffing": {{"destOverride": ["http", "tls"], "enabled": true}}, "tag": "socks"}}],
  "log": {{"loglevel": "warning"}},
  "outbounds": [
    {{"protocol": "shadowsocks", "settings": {{"servers": [{{"address": "{host}", "port": {port}, "method": "{cipher}", "password": "{password}"}}]}}, "streamSettings": {{"network": "ws", "wsSettings": {{"path": "/ss", "headers": {{"Host": "{host}"}}}}}}, "tag": "proxy"}},
    {{"protocol": "freedom", "tag": "direct"}},
    {{"protocol": "blackhole", "tag": "block"}}
  ],
  "remarks": "{host} (WS)",
  "routing": {{"domainStrategy": "IPIfNonMatch", "rules": [
    {{"outboundTag": "proxy", "port": "{allowed}", "type": "field"}},
    {{"outboundTag": "block", "port": "0-65535", "type": "field"}}
  ]}}
}}"#
    )
}

fn v2ray_ng_grpc_json(host: &str, port: u16, cipher: &str, password: &str, allowed: &str) -> String {
    format!(
        r#"{{
  "dns": {{"servers": ["1.1.1.1", "8.8.8.8"]}},
  "inbounds": [{{"listen": "127.0.0.1", "port": 10808, "protocol": "socks", "settings": {{"auth": "noauth", "udp": true}}, "sniffing": {{"destOverride": ["http", "tls"], "enabled": true}}, "tag": "socks"}}],
  "log": {{"loglevel": "warning"}},
  "outbounds": [
    {{"protocol": "shadowsocks", "settings": {{"servers": [{{"address": "{host}", "port": {port}, "method": "{cipher}", "password": "{password}"}}]}}, "streamSettings": {{"network": "grpc", "security": "tls", "grpcSettings": {{"serviceName": "GunService"}}, "tlsSettings": {{"serverName": "{host}", "allowInsecure": false}}}}, "tag": "proxy"}},
    {{"protocol": "freedom", "tag": "direct"}},
    {{"protocol": "blackhole", "tag": "block"}}
  ],
  "remarks": "{host} (gRPC+TLS)",
  "routing": {{"domainStrategy": "IPIfNonMatch", "rules": [
    {{"outboundTag": "proxy", "port": "{allowed}", "type": "field"}},
    {{"outboundTag": "block", "port": "0-65535", "type": "field"}}
  ]}}
}}"#
    )
}
