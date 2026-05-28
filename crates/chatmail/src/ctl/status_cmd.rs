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

//! `chatmail status` — Madmail `ctl/online.go`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime};

use chatmail_config::{
    load_config, port_from_listen, read_maddy_ast, Args, AppConfig, ConfigAst, Node,
};
use chatmail_db::passwords;
use chatmail_types::Result;
use serde::Deserialize;

use super::context::CtlContext;

const DEFAULT_RUNTIME_DIR: &str = "/run/madmail";
const STATUS_FILE: &str = "server_tracker.json";

#[derive(Debug, Clone)]
struct ServicePort {
    port: String,
    label: String,
    service: &'static str,
    proto: &'static str,
}

#[derive(Debug, Default)]
struct PortScan {
    ports: Vec<ServicePort>,
    runtime_dir: String,
    state_dir: String,
}

#[derive(Debug, Deserialize, Default)]
struct ServerTrackerStatus {
    boot_time: i64,
    unique_conn_ips: i32,
    unique_domains: i32,
    unique_ip_servers: i32,
}

struct ConnInfo {
    remote_addr: String,
}

pub async fn status(args: &Args, details: bool) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let config = if args.config.is_file() {
        load_config(&args.config)?
    } else {
        AppConfig::default()
    };

    let scan = parse_service_ports(&args.config, &config, &ctx.state_dir)?;
    let mut service_totals: HashMap<&str, i32> = HashMap::new();
    let mut service_ips: HashMap<&str, HashSet<String>> = HashMap::new();
    let mut port_results: Vec<(ServicePort, Vec<ConnInfo>)> = Vec::new();

    let mut known_turn_udp_ports = HashSet::new();
    for p in &scan.ports {
        if p.service == "TURN" && p.proto == "udp" {
            known_turn_udp_ports.insert(p.port.clone());
        }
    }

    for p in &scan.ports {
        let mut conns = established_connections(&p.port, p.proto);
        if p.service == "TURN" && p.proto == "udp" {
            let relay = turn_relay_count(&known_turn_udp_ports);
            for _ in 0..relay {
                conns.push(ConnInfo {
                    remote_addr: "relay".into(),
                });
            }
        }
        *service_totals.entry(p.service).or_default() += conns.len() as i32;
        let ips = service_ips.entry(p.service).or_default();
        for c in &conns {
            if c.remote_addr != "relay" {
                ips.insert(extract_ip(&c.remote_addr));
            }
        }
        port_results.push((p.clone(), conns));
    }

    for svc in ["IMAP", "TURN", "Shadowsocks"] {
        let Some(count) = service_totals.get(svc) else {
            continue;
        };
        let ips = service_ips.get(svc).map(|s| s.len()).unwrap_or(0);
        match svc {
            "TURN" => println!("{:<15} relays: {count}", svc),
            _ => println!("{:<15} connections: {count:<6} unique IPs: {ips}", svc),
        }
    }

    if details && !port_results.is_empty() {
        println!();
        println!("Per-port breakdown:");
        println!("{:<6}\t{:<5}\t{:<18}\t{:<12}\t{}", "PORT", "PROTO", "TYPE", "CONNECTIONS", "UNIQUE IPs");
        for (info, conns) in &port_results {
            let mut ips = HashSet::new();
            for c in conns {
                if c.remote_addr != "relay" {
                    ips.insert(extract_ip(&c.remote_addr));
                }
            }
            if info.service == "TURN" && info.proto == "udp" {
                println!(
                    "{}\t{}\t{}\t{} relays\t-",
                    info.port,
                    info.proto,
                    info.label,
                    conns.len()
                );
            } else {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    info.port,
                    info.proto,
                    info.label,
                    conns.len(),
                    ips.len()
                );
            }
        }
    }

    if let Ok(pool) = ctx.open_pool().await {
        let users = passwords::list_users(&pool).await?;
        println!();
        println!("Registered users:   {}", users.len());
    }

    if let Ok(st) = read_server_tracker(&scan.runtime_dir) {
        if st.boot_time > 0 {
            let boot = SystemTime::UNIX_EPOCH + Duration::from_secs(st.boot_time as u64);
            let uptime = SystemTime::now()
                .duration_since(boot)
                .unwrap_or_default()
                .as_secs();
            println!(
                "Boot time:          {} (up {})",
                format_boot_time(st.boot_time),
                format_uptime(uptime)
            );
        }
        if st.unique_conn_ips > 0 || st.unique_domains > 0 || st.unique_ip_servers > 0 {
            println!();
            println!("Email servers seen (since last restart):");
            println!("  Connection IPs:   {}", st.unique_conn_ips);
            println!("  Domain servers:   {}", st.unique_domains);
            println!("  IP servers:       {}", st.unique_ip_servers);
        }
    }

    Ok(())
}

fn parse_service_ports(
    config_path: &Path,
    cfg: &AppConfig,
    cli_state_dir: &Path,
) -> Result<PortScan> {
    let mut scan = PortScan {
        ports: Vec::new(),
        runtime_dir: cfg
            .runtime_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| DEFAULT_RUNTIME_DIR.into()),
        state_dir: cfg
            .state_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| cli_state_dir.display().to_string()),
    };

    if config_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(config_path) {
            if let Ok(ast) = read_maddy_ast(&content) {
                apply_ast_ports(&ast, &mut scan.ports);
                for node in &ast.nodes {
                    if node.name == "runtime_dir" && !node.args.is_empty() {
                        scan.runtime_dir = node.args[0].clone();
                    }
                    if node.name == "state_dir" && !node.args.is_empty() {
                        scan.state_dir = node.args[0].clone();
                    }
                }
            }
        }
    }

    push_listen_port(&mut scan.ports, cfg.imap_listen.as_deref(), "IMAP", "IMAP", "tcp");
    push_listen_port(
        &mut scan.ports,
        cfg.imap_tls_listen.as_deref(),
        "IMAP TLS",
        "IMAP",
        "tcp",
    );
    push_listen_port(
        &mut scan.ports,
        cfg.turn_listen_udp.as_deref(),
        "TURN UDP",
        "TURN",
        "udp",
    );
    push_listen_port(
        &mut scan.ports,
        cfg.turn_listen_tcp.as_deref(),
        "TURN TCP",
        "TURN",
        "tcp",
    );

    if !scan.ports.iter().any(|p| p.service == "IMAP") {
        scan.ports.push(ServicePort {
            port: "143".into(),
            label: "IMAP".into(),
            service: "IMAP",
            proto: "tcp",
        });
        scan.ports.push(ServicePort {
            port: "993".into(),
            label: "IMAP TLS".into(),
            service: "IMAP",
            proto: "tcp",
        });
    }

    dedupe_ports(&mut scan.ports);
    Ok(scan)
}

fn apply_ast_ports(ast: &ConfigAst, ports: &mut Vec<ServicePort>) {
    for node in &ast.nodes {
        walk_node(node, ports);
    }
}

fn walk_node(node: &Node, ports: &mut Vec<ServicePort>) {
    match node.name.as_str() {
        "imap" => {
            for (scheme, port) in endpoint_scheme_ports(&node.args) {
                let label = if scheme == "tls" {
                    "IMAP TLS"
                } else {
                    "IMAP"
                };
                push_port(ports, &port, label, "IMAP", "tcp");
            }
        }
        "chatmail" | "http" => {
            let mut alpn_imap = false;
            let mut ss_addr = None;
            if let Some(children) = &node.children {
                for child in children {
                    if child.name == "alpn_imap" {
                        alpn_imap = true;
                    }
                    if child.name == "ss_addr" && !child.args.is_empty() {
                        ss_addr = Some(child.args[0].trim_matches('"').to_string());
                    }
                }
            }
            if alpn_imap {
                for (_, port) in endpoint_scheme_ports(&node.args) {
                    push_port(ports, &port, "ALPN (chatmail)", "IMAP", "tcp");
                }
            }
            if let Some(addr) = ss_addr {
                if let Some(port) = addr.rsplit(':').next() {
                    push_port(ports, port, "Shadowsocks", "Shadowsocks", "tcp");
                }
            }
        }
        "turn" => {
            for (scheme, port) in endpoint_scheme_ports(&node.args) {
                if scheme == "udp" {
                    push_port(ports, &port, "TURN UDP", "TURN", "udp");
                } else {
                    push_port(ports, &port, "TURN TCP", "TURN", "tcp");
                }
            }
        }
        _ => {}
    }
    if let Some(children) = &node.children {
        for child in children {
            walk_node(child, ports);
        }
    }
}

fn endpoint_scheme_ports(args: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for arg in args {
        let Some((scheme, rest)) = arg.split_once("://") else {
            continue;
        };
        if let Some(port) = port_from_listen(Some(rest)) {
            out.push((scheme.to_string(), port));
        }
    }
    out
}

fn push_listen_port(
    ports: &mut Vec<ServicePort>,
    listen: Option<&str>,
    label: &str,
    service: &'static str,
    proto: &'static str,
) {
    if let Some(port) = port_from_listen(listen) {
        push_port(ports, &port, label, service, proto);
    }
}

fn push_port(
    ports: &mut Vec<ServicePort>,
    port: &str,
    label: &str,
    service: &'static str,
    proto: &'static str,
) {
    ports.push(ServicePort {
        port: port.to_string(),
        label: label.to_string(),
        service,
        proto,
    });
}

fn dedupe_ports(ports: &mut Vec<ServicePort>) {
    let mut seen = HashSet::new();
    ports.retain(|p| seen.insert(format!("{}/{}", p.port, p.proto)));
}

fn established_connections(port: &str, proto: &str) -> Vec<ConnInfo> {
    let output = if proto == "udp" {
        Command::new("ss")
            .args(["-unH", "sport", &format!("= :{port}")])
            .output()
    } else {
        Command::new("ss")
            .args([
                "-tnH",
                "state",
                "established",
                "sport",
                &format!("= :{port}"),
            ])
            .output()
    };
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_ss_output(&String::from_utf8_lossy(&output.stdout))
}

fn parse_ss_output(text: &str) -> Vec<ConnInfo> {
    let mut conns = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        let peer = fields[3];
        if peer == "*:*" || peer == "0.0.0.0:*" || peer == "[::]:*" {
            continue;
        }
        conns.push(ConnInfo {
            remote_addr: peer.to_string(),
        });
    }
    conns
}

fn turn_relay_count(known_turn_ports: &HashSet<String>) -> i32 {
    let Ok(output) = Command::new("ss").args(["-unap"]).output() else {
        return 0;
    };
    if !output.status.success() {
        return 0;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut count = 0i32;
    for line in text.lines() {
        if !line.contains("\"chatmail\"") && !line.contains("\"maddy\"") && !line.contains("\"madmail\"")
        {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 5 {
            continue;
        }
        let local_port = extract_port_from_addr(fields[3]);
        if known_turn_ports.contains(&local_port) {
            continue;
        }
        let peer = fields[4];
        if peer == "*:*" || peer == "0.0.0.0:*" || peer == "[::]:*" {
            count += 1;
            continue;
        }
        count += 1;
    }
    count
}

fn extract_port_from_addr(addr: &str) -> String {
    if let Some(rest) = addr.strip_prefix('[') {
        if let Some((_, port)) = rest.split_once("]:") {
            return port.to_string();
        }
    }
    addr.rsplit_once(':')
        .map(|(_, p)| p.to_string())
        .unwrap_or_else(|| addr.to_string())
}

fn extract_ip(addr: &str) -> String {
    if let Some(rest) = addr.strip_prefix('[') {
        if let Some(idx) = rest.find("]:") {
            return rest[..idx].to_string();
        }
        return rest.trim_matches(|c| c == '[' || c == ']').to_string();
    }
    if addr.matches(':').count() > 1 {
        if let Some((body, port)) = addr.rsplit_once(':') {
            if port.chars().all(|c| c.is_ascii_digit()) {
                return body.to_string();
            }
        }
        return addr.to_string();
    }
    addr.rsplit_once(':')
        .map(|(ip, _)| ip.to_string())
        .unwrap_or_else(|| addr.to_string())
}

fn read_server_tracker(runtime_dir: &str) -> Result<ServerTrackerStatus> {
    let path = Path::new(runtime_dir).join(STATUS_FILE);
    let data = std::fs::read_to_string(path).map_err(|e| chatmail_types::ChatmailError::Io(e))?;
    serde_json::from_str(&data).map_err(|e| chatmail_types::ChatmailError::config(e.to_string()))
}

fn format_boot_time(unix: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(unix)
        .ok()
        .and_then(|t| {
            t.format(
                &time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]")
                    .ok()?,
            )
            .ok()
        })
        .unwrap_or_else(|| unix.to_string())
}

fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;
    if days > 0 {
        format!("{days}d {hours}h {mins}m {s}s")
    } else if hours > 0 {
        format!("{hours}h {mins}m {s}s")
    } else if mins > 0 {
        format!("{mins}m {s}s")
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_ip_v4_and_v6() {
        assert_eq!(extract_ip("1.2.3.4:443"), "1.2.3.4");
        assert_eq!(extract_ip("[2001:db8::1]:443"), "2001:db8::1");
    }

    #[test]
    fn parse_ss_skips_wildcard_peer() {
        let out = parse_ss_output("0 0 0.0.0.0:143 1.2.3.4:999\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].remote_addr, "1.2.3.4:999");
    }
}
