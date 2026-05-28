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

use std::collections::HashMap;
use std::path::PathBuf;

use chatmail_types::wrap_ip_domain;

use crate::madmail_parse::{self, Node};
use crate::AppConfig;

/// Parse Madmail / Maddy `maddy.conf` using the same lexer/parser as
/// [`framework/cfgparser`](../../context/madmail/framework/cfgparser).
pub fn parse_maddy_conf_str(content: &str) -> AppConfig {
    parse_maddy_config(content).unwrap_or_default()
}

/// Parse `maddy.conf` and return an error if the file is syntactically invalid.
pub fn parse_maddy_config(content: &str) -> Result<AppConfig, madmail_parse::ParseError> {
    let ast = madmail_parse::read(content)?;
    Ok(apply_config(&ast.nodes, &ast.macros))
}

fn apply_config(nodes: &[Node], macros: &HashMap<String, Vec<String>>) -> AppConfig {
    let mut cfg = AppConfig::default();
    apply_macros(macros, &mut cfg);
    walk_nodes(nodes, &[], &mut cfg);
    if cfg.tls_mode.is_none() {
        cfg.tls_mode = detect_tls_mode(nodes);
    }
    if cfg.jit_domain.is_none() {
        cfg.jit_domain = cfg.primary_domain.clone();
    }
    if let Some(ref p) = cfg.primary_domain {
        cfg.primary_domain = Some(wrap_ip_domain(p));
    }
    if let Some(ref m) = cfg.mail_domain {
        cfg.mail_domain = Some(wrap_ip_domain(m));
    }
    if let Some(ref j) = cfg.jit_domain {
        cfg.jit_domain = Some(wrap_ip_domain(j));
    }
    cfg
}

fn apply_macros(macros: &HashMap<String, Vec<String>>, cfg: &mut AppConfig) {
    if let Some(v) = macro_first(macros, "hostname") {
        cfg.hostname = Some(v);
    }
    if let Some(v) = macro_first(macros, "primary_domain") {
        cfg.primary_domain = Some(v);
    }
    if let Some(v) = macro_join(macros, "local_domains") {
        cfg.local_domains = Some(v);
    }
    if let Some(v) = macro_first(macros, "public_ip") {
        cfg.public_ip = Some(v);
    }
}

fn macro_first(macros: &HashMap<String, Vec<String>>, key: &str) -> Option<String> {
    macros.get(key).and_then(|v| v.first().cloned())
}

fn macro_join(macros: &HashMap<String, Vec<String>>, key: &str) -> Option<String> {
    macros
        .get(key)
        .filter(|v| !v.is_empty())
        .map(|v| v.join(" "))
}

fn walk_nodes(nodes: &[Node], block_path: &[&str], cfg: &mut AppConfig) {
    for node in nodes {
        if let Some(children) = node.children.as_ref() {
            let mut path: Vec<&str> = block_path.to_vec();
            path.push(node.name.as_str());
            apply_endpoint_block(node, cfg);
            walk_nodes(children, &path, cfg);
            continue;
        }
        apply_directive(node.name.as_str(), &node.args, block_path, cfg);
    }
}

fn in_block<'a>(block_path: &[&'a str], name: &str) -> bool {
    block_path.iter().any(|b| *b == name || b.starts_with(name))
}

fn apply_endpoint_block(node: &Node, cfg: &mut AppConfig) {
    match node.name.as_str() {
        "smtp" => {
            for addr in endpoint_addrs(&node.args) {
                if cfg.smtp_listen.is_none() {
                    cfg.smtp_listen = Some(addr);
                    break;
                }
            }
        }
        "submission" => {
            for (scheme, addr) in endpoint_tokens(&node.args) {
                match scheme.as_str() {
                    "tls" if cfg.submission_tls_listen.is_none() => {
                        cfg.submission_tls_listen = Some(addr);
                    }
                    "tcp" if cfg.submission_listen.is_none() => {
                        cfg.submission_listen = Some(addr);
                    }
                    _ => {}
                }
            }
        }
        "imap" => {
            for (scheme, addr) in endpoint_tokens(&node.args) {
                match scheme.as_str() {
                    "tls" if cfg.imap_tls_listen.is_none() => {
                        cfg.imap_tls_listen = Some(addr);
                    }
                    "tcp" if cfg.imap_listen.is_none() => {
                        cfg.imap_listen = Some(addr);
                    }
                    _ => {}
                }
            }
        }
        "chatmail" | "http" => {
            for (scheme, addr) in endpoint_tokens(&node.args) {
                match scheme.as_str() {
                    "tls" if cfg.http_tls_listen.is_none() => {
                        cfg.http_tls_listen = Some(addr);
                    }
                    "tcp" if cfg.http_listen.is_none() => {
                        cfg.http_listen = Some(addr);
                    }
                    _ => {}
                }
            }
        }
        "turn" => {
            for (scheme, addr) in endpoint_tokens(&node.args) {
                match scheme.as_str() {
                    "udp" if cfg.turn_listen_udp.is_none() => cfg.turn_listen_udp = Some(addr),
                    "tcp" if cfg.turn_listen_tcp.is_none() => cfg.turn_listen_tcp = Some(addr),
                    _ => {}
                }
            }
        }
        "openmetrics" => {
            for addr in endpoint_addrs(&node.args) {
                if cfg.openmetrics_listen.is_none() {
                    cfg.openmetrics_listen = Some(addr);
                }
            }
        }
        _ => {}
    }
}

fn endpoint_tokens(args: &[String]) -> Vec<(String, String)> {
    args.iter()
        .filter_map(|t| {
            let scheme = t.split("://").next()?.to_string();
            maddy_listen_to_socket_addr(t).map(|addr| (scheme, addr))
        })
        .collect()
}

fn endpoint_addrs(args: &[String]) -> Vec<String> {
    args.iter()
        .filter_map(|t| maddy_listen_to_socket_addr(t))
        .collect()
}

fn apply_directive(name: &str, args: &[String], block_path: &[&str], cfg: &mut AppConfig) {
    let value = args.join(" ");
    let arg0 = args.first().map(String::as_str).unwrap_or("");
    let has_value = !value.is_empty();

    if block_path.is_empty() {
        match name {
            "state_dir" if has_value => cfg.state_dir = Some(value.clone().into()),
            "runtime_dir" if has_value => cfg.runtime_dir = Some(value.clone().into()),
            "debug" => cfg.debug = parse_bool(arg0),
            "log" if has_value => cfg.log_target = Some(value.clone()),
            "hostname" if has_value && cfg.hostname.is_none() => {
                cfg.hostname = Some(value.clone());
            }
            "tls" if arg0 == "file" => {
                cfg.tls_mode = Some("file".into());
                if args.len() >= 3 {
                    cfg.tls_cert_path = Some(args[1].clone().into());
                    cfg.tls_key_path = Some(args[2].clone().into());
                }
            }
            _ => {}
        }
    }

    if in_block(block_path, "auth.pass_table") && !in_block(block_path, "settings_table") {
        match name {
            "auto_create" => cfg.auth_auto_create = parse_bool(arg0),
            "jit_domain" if has_value => cfg.jit_domain = Some(value.clone()),
            "driver" if has_value => cfg.credentials_driver = Some(value.clone()),
            "dsn" if has_value => cfg.credentials_dsn = Some(strip_quotes(&value)),
            _ => {}
        }
    }

    if in_block(block_path, "storage.imapsql") {
        match name {
            "driver" if has_value => cfg.imapsql_driver = Some(value.clone()),
            "dsn" if has_value => cfg.imapsql_dsn = Some(strip_quotes(&value)),
            "default_quota" if has_value => cfg.default_quota = Some(value.clone()),
            "retention" if has_value => cfg.retention = Some(value.clone()),
            "unused_account_retention" if has_value => {
                cfg.unused_account_retention = Some(value.clone());
            }
            "appendlimit" if has_value => cfg.appendlimit = Some(value.clone()),
            _ => {}
        }
    }

    if in_block(block_path, "smtp") || in_block(block_path, "submission") {
        if name == "max_message_size" && has_value {
            cfg.max_message_size = Some(value.clone());
        }
    }

    if in_block(block_path, "target.queue") {
        match name {
            "max_tries" if has_value => {
                if let Ok(n) = arg0.parse::<u32>() {
                    cfg.queue.max_tries = n;
                }
            }
            "max_parallelism" if has_value => {
                if let Ok(n) = arg0.parse::<u32>() {
                    cfg.queue.max_parallelism = n.max(1);
                }
            }
            "location" if has_value => {
                cfg.queue.location = Some(value.clone().into());
            }
            "initial_retry" if has_value => {
                if let Ok(d) = parse_go_duration(arg0) {
                    cfg.queue.initial_retry_secs = d.as_secs();
                }
            }
            "retry_time_scale" if has_value => {
                if let Ok(f) = arg0.parse::<f64>() {
                    cfg.queue.retry_time_scale = f;
                }
            }
            "post_init_delay" if has_value => {
                if let Ok(d) = parse_go_duration(arg0) {
                    cfg.queue.post_init_delay_secs = d.as_secs();
                }
            }
            "max_delivery_time" | "delivery_timeout" if has_value => {
                if let Ok(d) = parse_go_duration(arg0) {
                    cfg.queue.max_delivery_secs = d.as_secs().max(1);
                }
            }
            _ => {}
        }
    }

    if in_block(block_path, "imap") {
        match name {
            "turn_enable" => cfg.turn_enable = parse_bool(arg0),
            "turn_server" if has_value => cfg.turn_server = Some(strip_quotes(&value)),
            "turn_port" if has_value => {
                if let Ok(n) = arg0.parse::<u16>() {
                    cfg.turn_port = n;
                }
            }
            "turn_secret" if has_value => cfg.turn_secret = Some(strip_quotes(&value)),
            "turn_ttl" if has_value => {
                if let Ok(n) = arg0.parse::<u64>() {
                    cfg.turn_ttl = n;
                }
            }
            "iroh_relay_url" if has_value => {
                cfg.iroh_relay_url = Some(strip_quotes(&value));
                cfg.iroh_enable = true;
            }
            _ => {}
        }
    }

    if in_block(block_path, "turn") {
        match name {
            "realm" if has_value => cfg.turn_realm = Some(strip_quotes(&value)),
            "secret" if has_value => {
                cfg.turn_secret = Some(strip_quotes(&value));
                cfg.turn_enable = true;
            }
            "relay_ip" if has_value => cfg.turn_relay_ip = Some(strip_quotes(&value)),
            "debug" => cfg.turn_debug = parse_bool(arg0),
            "test_force_relay" => cfg.turn_test_force_relay = parse_bool(arg0),
            _ => {}
        }
    }

    if in_block(block_path, "chatmail") {
        match name {
            "mail_domain" if has_value => {
                cfg.mail_domain = Some(value.clone());
                if cfg.primary_domain.is_none() {
                    cfg.primary_domain = Some(value.clone());
                }
            }
            "mx_domain" if has_value => cfg.mx_domain = Some(value.clone()),
            "public_ip" if has_value => cfg.public_ip = Some(value.clone()),
            "admin_path" if has_value => cfg.admin_path = Some(value.clone()),
            "admin_web_path" if has_value => cfg.admin_web_path = Some(value.clone()),
            "admin_token" if has_value => cfg.admin_token = Some(strip_quotes(&value)),
            "language" if has_value => cfg.language = Some(value.clone()),
            "www_dir" if has_value => {
                cfg.www_dir = Some(PathBuf::from(strip_quotes(&value)));
            }
            "username_length" if has_value => {
                if let Ok(n) = arg0.parse::<u32>() {
                    cfg.username_length = Some(n);
                }
            }
            "password_length" if has_value => {
                if let Ok(n) = arg0.parse::<u32>() {
                    cfg.password_length = Some(n);
                }
            }
            "min_username_length" if has_value => {
                if let Ok(n) = arg0.parse::<u32>() {
                    cfg.min_username_length = Some(n);
                }
            }
            "max_username_length" if has_value => {
                if let Ok(n) = arg0.parse::<u32>() {
                    cfg.max_username_length = Some(n);
                }
            }
            "password_min_length" if has_value => {
                if let Ok(n) = arg0.parse::<u32>() {
                    cfg.password_min_length = Some(n);
                }
            }
            "ss_addr" if has_value => cfg.ss_addr = Some(strip_quotes(&value)),
            "ss_password" if has_value => cfg.ss_password = Some(strip_quotes(&value)),
            "ss_cipher" if has_value => cfg.ss_cipher = Some(strip_quotes(&value)),
            "ss_cert" if has_value => cfg.ss_cert_path = Some(strip_quotes(&value).into()),
            "ss_key" if has_value => cfg.ss_key_path = Some(strip_quotes(&value).into()),
            "ss_allowed_ports" => {
                for p in args {
                    let p = strip_quotes(p);
                    if !p.is_empty() {
                        cfg.ss_allowed_ports.push(p);
                    }
                }
            }
            _ => {}
        }
    }
}

fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Go-style duration suffixes used by Madmail (`24h`, `7d`, `15m`, …).
pub fn parse_duration(s: &str) -> Result<std::time::Duration, ()> {
    let s = s.trim();
    if s.is_empty() || s == "0" {
        return Err(());
    }
    if let Some(d) = s.strip_suffix('d') {
        let n: u64 = d.trim().parse().map_err(|_| ())?;
        return Ok(std::time::Duration::from_secs(n * 86400));
    }
    if let Some(h) = s.strip_suffix('h') {
        let n: u64 = h.trim().parse().map_err(|_| ())?;
        return Ok(std::time::Duration::from_secs(n * 3600));
    }
    if let Some(m) = s.strip_suffix('m') {
        let n: u64 = m.trim().parse().map_err(|_| ())?;
        return Ok(std::time::Duration::from_secs(n * 60));
    }
    if let Some(sec) = s.strip_suffix('s') {
        let n: u64 = sec.trim().parse().map_err(|_| ())?;
        return Ok(std::time::Duration::from_secs(n));
    }
    let n: u64 = s.parse().map_err(|_| ())?;
    Ok(std::time::Duration::from_secs(n))
}

fn parse_go_duration(s: &str) -> Result<std::time::Duration, ()> {
    parse_duration(s)
}

fn parse_bool(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "1" | "true" | "on" | "yes"
    )
}

/// Convert `tcp://0.0.0.0:25` / `tls://0.0.0.0:465` to `0.0.0.0:25` (Madmail `config.Endpoint::Address`).
pub fn maddy_listen_to_socket_addr(token: &str) -> Option<String> {
    let token = token.trim();
    let rest = token
        .strip_prefix("tcp://")
        .or_else(|| token.strip_prefix("tls://"))?;
    Some(rest.to_string())
}

fn detect_tls_mode(nodes: &[Node]) -> Option<String> {
    fn walk(nodes: &[Node]) -> Option<String> {
        for node in nodes {
            if node.name == "tls" {
                if let Some(arg0) = node.args.first() {
                    if arg0 == "file" {
                        return Some("file".into());
                    }
                }
                if let Some(children) = node.children.as_ref() {
                    for child in children {
                        if child.name == "loader" {
                            if let Some(mode) = child.args.first() {
                                return Some(mode.clone());
                            }
                        }
                    }
                }
            }
            if let Some(children) = node.children.as_ref() {
                if let Some(mode) = walk(children) {
                    return Some(mode);
                }
            }
        }
        None
    }
    walk(nodes)
}

/// Resolve a DSN/path relative to `state_dir` (Madmail stores `credentials.db` there).
pub fn resolve_state_path(state_dir: &std::path::Path, path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        state_dir.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_maddy_listen_to_socket_addr() {
        assert_eq!(
            maddy_listen_to_socket_addr("tcp://0.0.0.0:25").as_deref(),
            Some("0.0.0.0:25")
        );
        assert_eq!(
            maddy_listen_to_socket_addr("tls://0.0.0.0:993").as_deref(),
            Some("0.0.0.0:993")
        );
    }

    #[test]
    fn test_parse_maddy_conf_full() {
        let content = r#"
$(hostname) = mail.example.org
$(primary_domain) = example.org
$(local_domains) = $(primary_domain) extra.org
$(public_ip) = 203.0.113.10
state_dir /var/lib/maddy
runtime_dir /run/maddy
log off
debug yes

auth.pass_table local_authdb {
    auto_create yes
    jit_domain $(primary_domain)
    table sql_table {
        driver sqlite3
        dsn credentials.db
        table_name passwords
    }
}

storage.imapsql local_mailboxes {
    driver sqlite3
    dsn imapsql.db
    default_quota 1G
    retention 24h
    appendlimit 100M
}

smtp tcp://0.0.0.0:25 {
}

submission tls://0.0.0.0:465 tcp://0.0.0.0:587 {
}

imap tls://0.0.0.0:993 tcp://0.0.0.0:143 {
}

chatmail tls://0.0.0.0:443 {
    mail_domain $(primary_domain)
    mx_domain $(hostname)
    public_ip $(public_ip)
    username_length 8
    password_length 16
    min_username_length 8
    max_username_length 20
    password_min_length 8
}

target.queue remote_queue {
    max_tries 8
    max_parallelism 4
    initial_retry 30m
    max_delivery_time 5m
}
"#;
        let cfg = parse_maddy_config(content).expect("parse");
        assert_eq!(cfg.hostname.as_deref(), Some("mail.example.org"));
        assert_eq!(cfg.primary_domain.as_deref(), Some("example.org"));
        assert_eq!(cfg.local_domains.as_deref(), Some("example.org extra.org"));
        assert_eq!(
            cfg.state_dir.as_deref(),
            Some(Path::new("/var/lib/maddy"))
        );
        assert_eq!(cfg.runtime_dir.as_deref(), Some(Path::new("/run/maddy")));
        assert!(cfg.auth_auto_create);
        assert_eq!(cfg.jit_domain.as_deref(), Some("example.org"));
        assert_eq!(cfg.credentials_driver.as_deref(), Some("sqlite3"));
        assert_eq!(cfg.credentials_dsn.as_deref(), Some("credentials.db"));
        assert_eq!(cfg.imapsql_dsn.as_deref(), Some("imapsql.db"));
        assert_eq!(cfg.default_quota.as_deref(), Some("1G"));
        assert_eq!(cfg.smtp_listen.as_deref(), Some("0.0.0.0:25"));
        assert_eq!(cfg.submission_listen.as_deref(), Some("0.0.0.0:587"));
        assert_eq!(cfg.submission_tls_listen.as_deref(), Some("0.0.0.0:465"));
        assert_eq!(cfg.imap_listen.as_deref(), Some("0.0.0.0:143"));
        assert_eq!(cfg.imap_tls_listen.as_deref(), Some("0.0.0.0:993"));
        assert_eq!(cfg.http_tls_listen.as_deref(), Some("0.0.0.0:443"));
        assert!(cfg.http_listen.is_none());
        assert_eq!(cfg.log_target.as_deref(), Some("off"));
        assert!(cfg.debug);
        assert_eq!(cfg.queue.max_tries, 8);
        assert_eq!(cfg.queue.max_parallelism, 4);
        assert_eq!(cfg.queue.initial_retry_secs, 30 * 60);
        assert_eq!(cfg.queue.max_delivery_secs, 5 * 60);
        assert_eq!(cfg.username_length, Some(8));
        assert_eq!(cfg.password_length, Some(16));
        assert_eq!(cfg.min_username_length, Some(8));
        assert_eq!(cfg.max_username_length, Some(20));
        assert_eq!(cfg.password_min_length, Some(8));
        let p = cfg.credential_policy();
        assert_eq!(p.generated_username_length(), 8);
        assert_eq!(p.generated_password_length(), 16);
    }

    #[test]
    fn parse_chatmail_credential_directives_only() {
        let content = r#"
chatmail tcp://0.0.0.0:80 {
    mail_domain example.org
    username_length 10
    password_length 20
    min_username_length 6
    max_username_length 18
    password_min_length 9
}
"#;
        let cfg = parse_maddy_config(content).unwrap();
        assert_eq!(cfg.username_length, Some(10));
        assert_eq!(cfg.password_length, Some(20));
        assert_eq!(cfg.min_username_length, Some(6));
        assert_eq!(cfg.max_username_length, Some(18));
        assert_eq!(cfg.password_min_length, Some(9));
        let p = cfg.credential_policy();
        assert_eq!(p.generated_username_length(), 10);
        assert_eq!(p.generated_password_length(), 20);
        assert_eq!(p.min_username_length, 6);
        assert_eq!(p.password_min_length, 9);
    }

    #[test]
    fn parse_bool_accepts_madmail_values() {
        let content = "auth.pass_table db {\nauto_create on\n}\n";
        let cfg = parse_maddy_config(content).unwrap();
        assert!(cfg.auth_auto_create);
    }

    #[test]
    fn quoted_postgres_dsn() {
        let content = r#"
storage.imapsql db {
    driver postgres
    dsn "host=127.0.0.1 port=5432 user=maddy dbname=maddy"
}
"#;
        let cfg = parse_maddy_config(content).unwrap();
        assert_eq!(cfg.imapsql_driver.as_deref(), Some("postgres"));
        assert_eq!(
            cfg.imapsql_dsn.as_deref(),
            Some("host=127.0.0.1 port=5432 user=maddy dbname=maddy")
        );
    }

    #[test]
    fn pass_table_postgres_sql_table() {
        let content = r#"
auth.pass_table local_authdb {
    table sql_table {
        driver postgres
        dsn "host=127.0.0.1 port=5432 user=test password=test dbname=test sslmode=disable"
        table_name passwords
    }
}
"#;
        let cfg = parse_maddy_config(content).unwrap();
        assert_eq!(cfg.credentials_driver.as_deref(), Some("postgres"));
        assert_eq!(
            cfg.credentials_dsn.as_deref(),
            Some("host=127.0.0.1 port=5432 user=test password=test dbname=test sslmode=disable")
        );
    }

    #[test]
    fn tls_file_block_sets_mode() {
        let content = "tls file /var/lib/maddy/certs/fullchain.pem /var/lib/maddy/certs/privkey.pem\n";
        let cfg = parse_maddy_config(content).unwrap();
        assert_eq!(cfg.tls_mode.as_deref(), Some("file"));
        assert_eq!(
            cfg.tls_cert_path.as_deref(),
            Some(Path::new("/var/lib/maddy/certs/fullchain.pem"))
        );
        assert_eq!(
            cfg.tls_key_path.as_deref(),
            Some(Path::new("/var/lib/maddy/certs/privkey.pem"))
        );
    }

    #[test]
    fn tls_loader_autocert() {
        let content = "tls {\n    loader autocert {\n        hostname mail.example.org\n    }\n}\n";
        let cfg = parse_maddy_config(content).unwrap();
        assert_eq!(cfg.tls_mode.as_deref(), Some("autocert"));
    }

    #[test]
    fn parse_ip_primary_domain_wraps_brackets() {
        let cfg = parse_maddy_config("$(primary_domain) = 1.1.1.1\n").unwrap();
        assert_eq!(cfg.primary_domain.as_deref(), Some("[1.1.1.1]"));
    }

    #[test]
    fn p9_ut03_parses_imap_turn_and_turn_endpoint() {
        let cfg = parse_maddy_config(
            r#"
$(hostname) = turn.example.org
$(primary_domain) = example.org
state_dir /var/lib/maddy

imap tls://0.0.0.0:993 {
    turn_enable on
    turn_server turn.example.org
    turn_port 3478
    turn_secret s3cr3t
    turn_ttl 86400
}

turn {
    realm example.org
    secret s3cr3t
    relay_ip 203.0.113.10
}
"#,
        )
        .expect("parse maddy");
        assert!(cfg.turn_enable);
        assert_eq!(cfg.turn_server.as_deref(), Some("turn.example.org"));
        assert_eq!(cfg.turn_port, 3478);
        assert_eq!(cfg.turn_secret.as_deref(), Some("s3cr3t"));
        assert_eq!(cfg.turn_ttl, 86400);
        assert_eq!(cfg.turn_realm.as_deref(), Some("example.org"));
        assert_eq!(cfg.turn_relay_ip.as_deref(), Some("203.0.113.10"));
    }

    #[test]
    fn parses_openmetrics_listen() {
        let cfg = parse_maddy_config("openmetrics tcp://127.0.0.1:9100 {\n}\n").unwrap();
        assert_eq!(
            cfg.openmetrics_listen.as_deref(),
            Some("127.0.0.1:9100")
        );
    }

    #[test]
    fn parses_madmail_reference_conf_globals() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/maddy_globals.conf");
        let content = std::fs::read_to_string(&path).expect("read fixture maddy.conf");
        let cfg = parse_maddy_config(&content).expect("parse reference maddy.conf");
        assert_eq!(cfg.primary_domain.as_deref(), Some("example.org"));
        assert_eq!(cfg.hostname.as_deref(), Some("mail.example.org"));
        assert_eq!(cfg.state_dir.as_deref(), Some(Path::new("/var/lib/maddy")));
        assert!(cfg.auth_auto_create);
        assert_eq!(cfg.credentials_dsn.as_deref(), Some("credentials.db"));
        assert_eq!(cfg.imapsql_dsn.as_deref(), Some("imapsql.db"));
        assert_eq!(cfg.retention.as_deref(), Some("24h"));
        assert_eq!(cfg.tls_mode.as_deref(), Some("file"));
        assert_eq!(cfg.log_target.as_deref(), Some("stderr"));
    }
}
