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
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use chatmail_config::{effective_app_db_path, port_from_listen, AppConfig};
use chatmail_db::{
    db_fetch_scalar, get_setting, load_mail_port_overrides, message_stats_snapshot, passwords,
    settings_keys, DbPool,
};
use chatmail_imap::imap_connection_peers;
use chatmail_push::{
    consecutive_failures, push_mode, push_runtime_enabled, push_stats_snapshot,
    AUTO_DISABLE_AFTER_FAILURES,
};
use chatmail_state::tracker::FederationStatRow;

use super::AdminResult;
use crate::AdminState;

static BOOT_TIME: std::sync::OnceLock<SystemTime> = std::sync::OnceLock::new();

fn boot_time() -> SystemTime {
    *BOOT_TIME.get_or_init(SystemTime::now)
}

pub async fn status(st: &AdminState, method: &str) -> AdminResult {
    if method != "GET" {
        return Err((405, format!("method {method} not allowed, use GET")));
    }
    let body = build_status_body(st).await?;
    Ok((200, Some(Value::Object(body))))
}

/// Dashboard overview: status metrics plus host disk capacity and registration-token count.
pub async fn overview(st: &AdminState, method: &str) -> AdminResult {
    if method != "GET" {
        return Err((405, format!("method {method} not allowed, use GET")));
    }
    let mut body = build_status_body(st).await?;
    body.insert(
        "disk".into(),
        disk_usage(&st.state_dir).unwrap_or_else(|| {
            json!({
                "total_bytes": 0,
                "used_bytes": 0,
                "available_bytes": 0,
                "percent_used": 0.0,
            })
        }),
    );
    let token_total = count_registration_tokens(&st.pool).await?;
    body.insert("tokens".into(), json!({ "total": token_total }));
    let (_, settings_val) = super::settings::all_settings(st, "GET").await?;
    if let Some(Value::Object(settings)) = settings_val {
        body.insert("settings".into(), Value::Object(settings));
    }
    Ok((200, Some(Value::Object(body))))
}

async fn build_status_body(
    st: &AdminState,
) -> Result<serde_json::Map<String, Value>, (u16, String)> {
    let users = passwords::list_users(&st.pool).await.map_err(db_err)?;
    let boot = boot_time();
    let duration = boot.elapsed().unwrap_or_default();
    let boot_secs = boot
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let boot_time = format_boot_time_rfc3339(boot_secs);

    let (sent, outbound, received) = message_stats_snapshot();
    let turn_port = setting_port(&st.pool, settings_keys::TURN_PORT, "3478").await;
    // Live IMAP sessions tracked in-process (on_open/on_close). Do not merge with `ss`:
    // `ss` counts raw TCP sockets and was previously mixed via max()/union(), producing
    // inconsistent connection vs unique-IP counts.
    let (proc_conns, proc_ips) = imap_connection_peers();
    let (imap_conns, imap_ips) = if proc_conns > 0 || !proc_ips.is_empty() {
        (proc_conns, proc_ips.len() as i32)
    } else {
        let ports = imap_listen_ports(st, &st.pool).await;
        let (ss_conns, ss_ips) = count_tcp_on_ports(&ports);
        (ss_conns, ss_ips.len() as i32)
    };
    let turn_relays = count_turn_relays(&turn_port);
    // Shadowsocks is not implemented — always report zero (do not probe `ss` on SS port).
    let (ss_conns, ss_ips) = (0, 0);

    let mut body = serde_json::Map::new();
    body.insert("version".into(), json!(st.version));
    body.insert(
        "imap".into(),
        json!({ "connections": imap_conns, "unique_ips": imap_ips }),
    );
    body.insert("turn".into(), json!({ "relays": turn_relays }));
    body.insert(
        "shadowsocks".into(),
        json!({ "connections": ss_conns, "unique_ips": ss_ips }),
    );
    body.insert("users".into(), json!({ "registered": users.len() }));
    body.insert(
        "uptime".into(),
        json!({
            "boot_time": boot_time,
            "duration": format_duration(duration),
        }),
    );
    body.insert("sent_messages".into(), json!(sent));
    body.insert("outbound_messages".into(), json!(outbound));
    body.insert("received_messages".into(), json!(received));
    let local = local_hostnames(st).await;
    let rows = st.app.federation_tracker.snapshot();
    if let Some(es) = email_servers_json_from_rows(&rows, &local) {
        body.insert("email_servers".into(), es);
    }
    if let Some(ft) = federation_traffic_json(&rows, &local) {
        body.insert("federation_traffic".into(), ft);
    }
    let mr = chatmail_db::message_retention_status(&st.pool)
        .await
        .map_err(db_err)?;
    body.insert(
        "message_retention".into(),
        json!({
            "enabled": mr.enabled,
            "days": mr.days,
            "retention": mr.retention,
        }),
    );
    let push_enabled = push_runtime_enabled(&st.pool).await.map_err(db_err)?;
    let push_mode = push_mode(&st.pool).await.map_err(db_err)?;
    body.insert(
        "push".into(),
        json!({
            "enabled": push_enabled,
            "mode": push_mode.as_str(),
            "successful_notifications": push_stats_snapshot(),
            "consecutive_failures": consecutive_failures(),
            "auto_disable_after": AUTO_DISABLE_AFTER_FAILURES,
        }),
    );

    Ok(body)
}

async fn count_registration_tokens(pool: &DbPool) -> Result<i64, (u16, String)> {
    db_fetch_scalar!(pool, i64, "SELECT COUNT(*) FROM registration_tokens").map_err(db_err)
}

/// Ports where madmail-v2 IMAP may listen (Madmail uses `__IMAP_PORT__`, not only TLS).
async fn imap_listen_ports(st: &AdminState, pool: &DbPool) -> Vec<String> {
    let mut ports = Vec::new();
    let mut add = |p: Option<String>| {
        if let Some(p) = p.filter(|s| !s.trim().is_empty()) {
            if !ports.iter().any(|x| x == &p) {
                ports.push(p);
            }
        }
    };
    // Effective bound port from supervisor (e.g. 1143 when DB has no __IMAP_PORT__).
    let snap = st.app.listener_ports.snapshot();
    if !snap.imap_plain_port.is_empty() {
        add(Some(snap.imap_plain_port.clone()));
    }
    if !snap.imap_tls_port.is_empty() {
        add(Some(snap.imap_tls_port.clone()));
    }
    if let Ok(db) = load_mail_port_overrides(pool).await {
        add(db.imap_port);
        add(db.imap_tls_port);
    }
    add(get_setting(pool, settings_keys::IMAP_PORT)
        .await
        .ok()
        .flatten());
    add(get_setting(pool, settings_keys::IMAP_TLS_PORT)
        .await
        .ok()
        .flatten());
    if let Ok(addr) = std::env::var("CHATMAIL_IMAP_ADDR") {
        if let Some(p) = port_from_listen(Some(&addr)) {
            add(Some(p.to_string()));
        }
    }
    if ports.is_empty() {
        ports.push("1143".into());
        ports.push("143".into());
        ports.push("993".into());
    }
    ports
}

fn federation_peer_key(domain: &str) -> String {
    domain
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_lowercase()
}

fn is_successful_federation_peer(row: &FederationStatRow) -> bool {
    row.successful_deliveries > 0 || row.inbound_deliveries > 0
}

/// Count successful federated peer servers: total, domain-named, and IP-literal peers.
fn email_servers_counts(
    rows: &[FederationStatRow],
    local_hostnames: &HashSet<String>,
) -> Option<(i32, i32, i32)> {
    let successful: Vec<_> = rows
        .iter()
        .filter(|r| is_successful_federation_peer(r))
        .filter(|r| !local_hostnames.contains(&federation_peer_key(&r.domain)))
        .collect();
    if successful.is_empty() {
        return None;
    }
    let ip_servers = successful
        .iter()
        .filter(|r| is_ip_like_domain(&r.domain))
        .count() as i32;
    let connections = successful.len() as i32;
    let domain_servers = connections - ip_servers;
    Some((connections, domain_servers, ip_servers))
}

async fn local_hostnames(st: &AdminState) -> HashSet<String> {
    let mut names = HashSet::new();
    if let Ok(Some(h)) = get_setting(&st.pool, settings_keys::SMTP_HOSTNAME).await {
        let h = h.trim();
        if !h.is_empty() {
            names.insert(federation_peer_key(h));
        }
    }
    names
}

fn email_servers_json_from_rows(
    rows: &[FederationStatRow],
    local: &HashSet<String>,
) -> Option<Value> {
    let (connections, domain_servers, ip_servers) = email_servers_counts(rows, local)?;
    Some(json!({
        "connections": connections,
        // Deprecated alias kept for older admin-web builds.
        "connection_ips": connections,
        "domain_servers": domain_servers,
        "ip_servers": ip_servers,
    }))
}

fn row_failed(row: &FederationStatRow) -> i64 {
    row.failed_http + row.failed_https + row.failed_smtp
}

fn row_success_transport(row: &FederationStatRow) -> i64 {
    row.success_http + row.success_https + row.success_smtp
}

fn row_attempts(row: &FederationStatRow) -> i64 {
    row_success_transport(row) + row_failed(row)
}

fn row_mean_latency_ms(row: &FederationStatRow) -> f64 {
    if row.successful_deliveries > 0 {
        row.total_latency_ms as f64 / row.successful_deliveries as f64
    } else {
        0.0
    }
}

fn classify_federation_peer(row: &FederationStatRow) -> Option<&'static str> {
    let failed = row_failed(row);
    let attempts = row_attempts(row);
    let has_activity = attempts > 0 || row.successful_deliveries > 0 || row.inbound_deliveries > 0;
    if !has_activity {
        return None;
    }
    if failed == 0 {
        return Some("perfect");
    }
    if attempts == 0 {
        return Some("bad");
    }
    let rate = row_success_transport(row) as f64 / attempts as f64;
    if rate < 0.3 {
        Some("bad")
    } else {
        Some("federated")
    }
}

/// Aggregated federation delivery metrics for the admin dashboard (`GET /admin/status`).
fn federation_traffic_json(rows: &[FederationStatRow], local: &HashSet<String>) -> Option<Value> {
    let mut inbound = 0i64;
    let mut outbound = 0i64;
    let mut queued = 0i64;
    let mut expired = 0i64;
    let mut latency_sum = 0f64;
    let mut latency_count = 0i64;
    let mut perfect = 0i64;
    let mut federated = 0i64;
    let mut bad = 0i64;
    let mut peers = 0i64;

    for row in rows {
        if local.contains(&federation_peer_key(&row.domain)) {
            continue;
        }
        peers += 1;
        inbound += row.inbound_deliveries;
        outbound += row.successful_deliveries;
        queued += row.queued_messages;
        expired += row_failed(row);
        let mean = row_mean_latency_ms(row);
        if mean > 0.0 {
            latency_sum += mean;
            latency_count += 1;
        }
        match classify_federation_peer(row) {
            Some("perfect") => perfect += 1,
            Some("federated") => federated += 1,
            Some("bad") => bad += 1,
            _ => {}
        }
    }

    if peers == 0 {
        return None;
    }

    let mean_latency_ms = if latency_count > 0 {
        (latency_sum / latency_count as f64).round() as i64
    } else {
        0
    };

    Some(json!({
        "inbound": inbound,
        "outbound": outbound,
        "queued": queued,
        "expired": expired,
        "mean_latency_ms": mean_latency_ms,
        "health": {
            "perfect": perfect,
            "federated": federated,
            "bad": bad,
        },
    }))
}

fn is_ip_like_domain(domain: &str) -> bool {
    let bare = domain.trim().trim_matches(|c| c == '[' || c == ']');
    chatmail_types::is_ipv4_literal(bare)
}

async fn setting_port(pool: &DbPool, key: &str, default: &str) -> String {
    match get_setting(pool, key).await {
        Ok(Some(v)) if !v.trim().is_empty() => v,
        _ => default.to_string(),
    }
}

/// Sum established TCP connections on local IMAP ports (`ss` fallback).
fn count_tcp_on_ports(ports: &[String]) -> (i32, std::collections::HashSet<String>) {
    let mut ips = std::collections::HashSet::new();
    let mut connections = 0i32;
    for port in ports {
        let (c, port_ips) = count_tcp_connections(port);
        connections += c;
        ips.extend(port_ips);
    }
    (connections, ips)
}

/// Established TCP connections on `sport = :port` via `ss` (Madmail `countTCPConnections`).
fn count_tcp_connections(port: &str) -> (i32, std::collections::HashSet<String>) {
    let output = match std::process::Command::new("ss")
        .args([
            "-tnH",
            "state",
            "established",
            "sport",
            &format!("= :{port}"),
        ])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return (0, std::collections::HashSet::new()),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut ips = std::collections::HashSet::new();
    let mut connections = 0i32;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 5 {
            continue;
        }
        let peer = fields[4];
        if peer == "*:*" || peer == "0.0.0.0:*" || peer == "[::]:*" {
            continue;
        }
        connections += 1;
        if let Some(ip) = extract_ip_from_addr(peer) {
            ips.insert(ip);
        }
    }
    (connections, ips)
}

fn count_turn_relays(known_port: &str) -> i32 {
    let output = match std::process::Command::new("ss").args(["-unap"]).output() {
        Ok(o) if o.status.success() => o,
        _ => return 0,
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut count = 0i32;
    for line in text.lines() {
        if !line.contains("\"chatmail\"") && !line.contains("\"maddy\"") {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 5 {
            continue;
        }
        let local_port = extract_port_from_addr(fields[3]);
        if local_port == known_port {
            continue;
        }
        count += 1;
    }
    count
}

fn extract_ip_from_addr(addr: &str) -> Option<String> {
    if let Some(rest) = addr.strip_prefix('[') {
        let idx = rest.find("]:")?;
        return Some(rest[..idx].to_string());
    }
    if addr.matches(':').count() > 1 {
        return Some(addr.to_string());
    }
    addr.rsplit_once(':').map(|(ip, _)| ip.to_string())
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

fn format_boot_time_rfc3339(secs: u64) -> String {
    time::OffsetDateTime::from_unix_timestamp(secs as i64)
        .ok()
        .and_then(|t| {
            t.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| secs.to_string())
}

pub async fn storage(st: &AdminState, method: &str) -> AdminResult {
    if method != "GET" {
        return Err((405, format!("method {method} not allowed, use GET")));
    }
    let state_size = dir_size(&st.state_dir).await;
    let db_path = effective_app_db_path(&st.state_dir, &AppConfig::default());
    let db_size = tokio::fs::metadata(&db_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let mut body = serde_json::Map::new();
    if let Some(disk) = disk_usage(&st.state_dir) {
        body.insert("disk".into(), disk);
    }
    body.insert(
        "state_dir".into(),
        json!({
            "path": st.state_dir.display().to_string(),
            "size_bytes": state_size as i64,
        }),
    );
    body.insert(
        "database".into(),
        json!({ "driver": "sqlite3", "size_bytes": db_size as i64 }),
    );

    Ok((200, Some(serde_json::Value::Object(body))))
}

#[cfg(unix)]
fn disk_usage(path: &Path) -> Option<serde_json::Value> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    let path_str = path.to_str()?;
    let cpath = CString::new(path_str).ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(cpath.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    let bsize = stat.f_frsize;
    let total_bytes = stat.f_blocks * bsize;
    let avail_bytes = stat.f_bavail * bsize;
    let used_bytes = total_bytes.saturating_sub(avail_bytes);
    let percent_used = if total_bytes > 0 {
        used_bytes as f64 / total_bytes as f64 * 100.0
    } else {
        0.0
    };
    Some(json!({
        "total_bytes": total_bytes,
        "used_bytes": used_bytes,
        "available_bytes": avail_bytes,
        "percent_used": percent_used,
    }))
}

#[cfg(not(unix))]
fn disk_usage(_path: &Path) -> Option<serde_json::Value> {
    None
}

pub fn restart(method: &str) -> AdminResult {
    if method != "POST" {
        return Err((405, "use POST".into()));
    }
    tracing::warn!("admin requested restart (not implemented — restart chatmail service manually)");
    Ok((
        200,
        Some(json!({
            "status": "restarting",
            "message": "Restart not automated in madmail-v2; restart the service unit manually."
        })),
    ))
}

#[derive(serde::Deserialize, Default)]
struct ReloadBody {
    /// `full` (default) or `http` — remount admin-web routes only.
    #[serde(default)]
    scope: Option<String>,
    /// Block until reload finishes (recommended for admin-web path changes).
    #[serde(default)]
    wait: bool,
}

pub async fn reload(st: &AdminState, method: &str, body: &serde_json::Value) -> AdminResult {
    if method != "POST" {
        return Err((405, "use POST".into()));
    }
    let req: ReloadBody = serde_json::from_value(body.clone()).unwrap_or_default();
    let scope = match req
        .scope
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some("http") | Some("http_routes") | Some("routes") => {
            chatmail_state::ReloadScope::HttpRoutes
        }
        Some("full") | None => chatmail_state::ReloadScope::Full,
        Some(other) => {
            return Err((400, format!("invalid scope: {other} (expected full|http)")));
        }
    };
    super::toggles::queue_reload(st, scope, req.wait).await?;
    let message = match scope {
        chatmail_state::ReloadScope::HttpRoutes => {
            "HTTP routes remounted (admin API, admin-web, www)."
        }
        chatmail_state::ReloadScope::Full => {
            "Stopping listeners, reloading caches from DB, and rebinding SMTP/IMAP/HTTP ports."
        }
    };
    Ok((
        200,
        Some(json!({
            "status": if req.wait { "reloaded" } else { "reloading" },
            "message": message,
        })),
    ))
}

async fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(mut rd) = tokio::fs::read_dir(&p).await else {
            continue;
        };
        while let Ok(Some(ent)) = rd.next_entry().await {
            let Ok(meta) = ent.metadata().await else {
                continue;
            };
            if meta.is_dir() {
                stack.push(ent.path());
            } else {
                total += meta.len();
            }
        }
    }
    total
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;
    if days > 0 {
        format!("{days}d {hours}h {mins}m {s}s")
    } else if hours > 0 {
        format!("{hours}h {mins}m {s}s")
    } else {
        format!("{mins}m {s}s")
    }
}

pub fn db_err(e: impl std::fmt::Display) -> (u16, String) {
    (500, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_tcp_connections_uses_peer_address() {
        let ss_line = "ESTAB 0 0 10.0.0.1:993 192.168.1.5:54321\n";
        let output = std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: ss_line.as_bytes().to_vec(),
            stderr: Vec::new(),
        };
        // Exercise the same parsing logic as count_tcp_connections without invoking `ss`.
        let text = String::from_utf8_lossy(&output.stdout);
        let mut ips = std::collections::HashSet::new();
        let mut connections = 0i32;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 5 {
                continue;
            }
            let peer = fields[4];
            if peer == "*:*" || peer == "0.0.0.0:*" || peer == "[::]:*" {
                continue;
            }
            connections += 1;
            if let Some(ip) = extract_ip_from_addr(peer) {
                ips.insert(ip);
            }
        }
        assert_eq!(connections, 1);
        assert_eq!(ips.len(), 1);
        assert!(ips.contains("192.168.1.5"));
    }

    fn sample_row(domain: &str, successful: i64, inbound: i64) -> FederationStatRow {
        FederationStatRow {
            domain: domain.into(),
            queued_messages: 0,
            failed_http: 0,
            failed_https: 0,
            failed_smtp: 0,
            success_http: 0,
            success_https: 0,
            success_smtp: 0,
            inbound_deliveries: inbound,
            successful_deliveries: successful,
            total_latency_ms: 0,
            last_active: 0,
        }
    }

    #[test]
    fn email_servers_counts_successful_peers_only() {
        let rows = vec![
            sample_row("mail.example.org", 3, 0),
            sample_row("192.168.1.10", 1, 0),
            sample_row("failed.example.org", 0, 0),
            sample_row("inbound-only.test", 0, 2),
        ];
        let local = HashSet::new();
        let (connections, domain_servers, ip_servers) =
            email_servers_counts(&rows, &local).unwrap();
        assert_eq!(connections, 3);
        assert_eq!(domain_servers, 2);
        assert_eq!(ip_servers, 1);
    }

    #[test]
    fn email_servers_counts_excludes_local_hostname() {
        let rows = vec![sample_row("mail.local.test", 1, 0)];
        let local = HashSet::from([federation_peer_key("mail.local.test")]);
        assert!(email_servers_counts(&rows, &local).is_none());
    }

    fn row_with_failures(domain: &str, success: i64, failed: i64) -> FederationStatRow {
        FederationStatRow {
            domain: domain.into(),
            queued_messages: 0,
            failed_http: failed,
            failed_https: 0,
            failed_smtp: 0,
            success_http: success,
            success_https: 0,
            success_smtp: 0,
            inbound_deliveries: 0,
            successful_deliveries: success,
            total_latency_ms: success * 100,
            last_active: 0,
        }
    }

    #[test]
    fn federation_traffic_json_aggregates_peers() {
        let rows = vec![
            sample_row("good.test", 5, 2),
            row_with_failures("bad.test", 1, 9),
            row_with_failures("ok.test", 4, 1),
        ];
        let local = HashSet::new();
        let v = federation_traffic_json(&rows, &local).unwrap();
        assert_eq!(v.get("inbound").and_then(|x| x.as_i64()), Some(2));
        assert_eq!(v.get("outbound").and_then(|x| x.as_i64()), Some(10));
        assert_eq!(
            v.get("health")
                .and_then(|h| h.get("perfect"))
                .and_then(|x| x.as_i64()),
            Some(1)
        );
        assert_eq!(
            v.get("health")
                .and_then(|h| h.get("federated"))
                .and_then(|x| x.as_i64()),
            Some(1)
        );
        assert_eq!(
            v.get("health")
                .and_then(|h| h.get("bad"))
                .and_then(|x| x.as_i64()),
            Some(1)
        );
    }
}
