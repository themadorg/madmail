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

//! Mail client connection hints for `dclogin:` / www templates (Madmail parity).

use std::path::{Path, PathBuf};

use crate::AppConfig;

/// Admin DB overrides for listener ports and dclogin security (`__SMTP_PORT__`, …).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DbMailPorts {
    pub smtp_port: Option<String>,
    pub submission_port: Option<String>,
    pub submission_tls_port: Option<String>,
    pub imap_port: Option<String>,
    pub imap_tls_port: Option<String>,
    pub dclogin_imap_security: Option<String>,
    pub dclogin_smtp_security: Option<String>,
    pub http_port: Option<String>,
    pub https_port: Option<String>,
}

/// Addresses the running process actually bound (from supervisor).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeListeners {
    pub imap_plain_addr: Option<String>,
    pub imap_tls_addr: Option<String>,
    pub submission_plain_addr: Option<String>,
    pub submission_tls_addr: Option<String>,
    /// Inbound SMTP (port 25), not submission.
    pub smtp_addr: Option<String>,
    pub http_plain_addr: Option<String>,
    pub http_tls_addr: Option<String>,
}

/// Ports and socket modes exposed to Delta Chat via the registration page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcloginMailSettings {
    /// Host clients should connect to (HTTP Host, public IP, or mail domain).
    pub client_host: String,
    pub imap_port_tls: String,
    pub imap_port_starttls: String,
    pub smtp_port_tls: String,
    pub smtp_port_starttls: String,
    /// One of: `ssl`, `starttls`, `default`, `plain` (Delta Chat dclogin `is` / `ss`).
    pub dclogin_imap_security: String,
    pub dclogin_smtp_security: String,
}

impl DcloginMailSettings {
    pub fn from_config(config: &AppConfig, http_host: Option<&str>) -> Self {
        Self::from_config_with_db(config, http_host, &DbMailPorts::default())
    }

    /// File/env defaults, overridden by DB values when set (Madmail `hydrateCache`).
    pub fn from_config_with_db(
        config: &AppConfig,
        http_host: Option<&str>,
        db: &DbMailPorts,
    ) -> Self {
        Self::from_config_with_db_and_runtime(config, http_host, db, None)
    }

    pub fn from_config_with_db_and_runtime(
        config: &AppConfig,
        http_host: Option<&str>,
        db: &DbMailPorts,
        runtime: Option<&RuntimeListeners>,
    ) -> Self {
        let client_host = client_connect_host(config, http_host);

        let (imap_plain, has_imap_plain) = resolve_port(
            db.imap_port.as_deref(),
            config.imap_listen.as_deref(),
            std::env::var("CHATMAIL_IMAP_ADDR").ok().as_deref(),
            "143",
        );
        let (imap_tls, has_imap_tls) = resolve_port(
            db.imap_tls_port.as_deref(),
            config.imap_tls_listen.as_deref(),
            None,
            "993",
        );

        let (smtp_plain, has_smtp_plain) = resolve_port(
            db.submission_port.as_deref(),
            config
                .submission_listen
                .as_deref()
                .or(config.smtp_listen.as_deref()),
            std::env::var("CHATMAIL_SMTP_ADDR").ok().as_deref(),
            "587",
        );
        let (smtp_tls, has_smtp_tls) = resolve_port(
            db.submission_tls_port.as_deref(),
            config.submission_tls_listen.as_deref(),
            None,
            "465",
        );

        // Inbound SMTP port in admin (`__SMTP_PORT__`) — used when no submission port is set.
        let (smtp_plain, has_smtp_plain) =
            if db.submission_port.as_deref().is_none_or(|s| s.is_empty())
                && db.smtp_port.as_deref().is_some_and(|s| !s.is_empty())
            {
                (db.smtp_port.clone().unwrap(), true)
            } else {
                (smtp_plain, has_smtp_plain)
            };

        let (dclogin_imap_security, imap_port_tls, imap_port_starttls) =
            if let Some(sec) = non_empty(db.dclogin_imap_security.as_deref()) {
                (sec.to_string(), imap_tls.clone(), imap_plain.clone())
            } else if let Some(rt) = runtime {
                let rt_has_tls = rt.imap_tls_addr.is_some();
                let rt_has_plain = rt.imap_plain_addr.is_some();
                if rt_has_tls || rt_has_plain {
                    let (tls_p, plain_p) = runtime_mail_ports(
                        rt.imap_tls_addr.as_deref(),
                        rt.imap_plain_addr.as_deref(),
                        &imap_tls,
                        &imap_plain,
                    );
                    dclogin_ports(rt_has_tls, rt_has_plain, &tls_p, &plain_p)
                } else {
                    dclogin_ports(has_imap_tls, has_imap_plain, &imap_tls, &imap_plain)
                }
            } else {
                dclogin_ports(has_imap_tls, has_imap_plain, &imap_tls, &imap_plain)
            };

        let (dclogin_smtp_security, smtp_port_tls, smtp_port_starttls) =
            if let Some(sec) = non_empty(db.dclogin_smtp_security.as_deref()) {
                (sec.to_string(), smtp_tls.clone(), smtp_plain.clone())
            } else if let Some(rt) = runtime {
                let rt_has_tls = rt.submission_tls_addr.is_some();
                let rt_has_plain = rt.submission_plain_addr.is_some();
                if rt_has_tls || rt_has_plain {
                    let (tls_p, plain_p) = runtime_mail_ports(
                        rt.submission_tls_addr.as_deref(),
                        rt.submission_plain_addr.as_deref(),
                        &smtp_tls,
                        &smtp_plain,
                    );
                    dclogin_ports(rt_has_tls, rt_has_plain, &tls_p, &plain_p)
                } else {
                    // Inbound SMTP (25) is not used for Delta Chat — only bound submission ports.
                    dclogin_ports(has_smtp_tls, has_smtp_plain, &smtp_tls, &smtp_plain)
                }
            } else {
                dclogin_ports(has_smtp_tls, has_smtp_plain, &smtp_tls, &smtp_plain)
            };

        Self {
            client_host,
            imap_port_tls,
            imap_port_starttls,
            smtp_port_tls,
            smtp_port_starttls,
            dclogin_imap_security,
            dclogin_smtp_security,
        }
    }
}

fn first_port(candidates: impl IntoIterator<Item = Option<String>>, default: &str) -> String {
    for c in candidates {
        if let Some(p) = c.filter(|s| !s.trim().is_empty()) {
            return p;
        }
    }
    default.to_string()
}

/// Effective `host:port` for the madmail-v2 SMTP listener at process start.
///
/// Matches Madmail admin **SMTP port** (`__SMTP_PORT__` / `smtp tcp://…` in maddy.conf), not
/// submission. DB value wins over the config file so `smtp tcp://0.0.0.0:25` does not stick
/// when admin sets `2525`.
pub fn effective_smtp_listen(config: &AppConfig, db: &DbMailPorts) -> String {
    let host = listen_host(config.smtp_listen.as_deref());
    let port = first_port(
        [
            db.smtp_port.clone(),
            port_from_listen(config.smtp_listen.as_deref()),
            port_from_listen(std::env::var("CHATMAIL_SMTP_ADDR").ok().as_deref()),
        ],
        "25",
    );
    format!("{host}:{port}")
}

/// Submission STARTTLS/plain (`submission tcp://…` / `__SUBMISSION_PORT__`).
pub fn effective_submission_plain_listen(config: &AppConfig, db: &DbMailPorts) -> Option<String> {
    let has_conf = config.submission_listen.is_some();
    let has_db = db.submission_port.as_deref().is_some_and(|s| !s.is_empty());
    if !has_conf && !has_db {
        return None;
    }
    let host = listen_host(
        config
            .submission_listen
            .as_deref()
            .or(config.submission_tls_listen.as_deref()),
    );
    let port = first_port(
        [
            db.submission_port.clone(),
            port_from_listen(config.submission_listen.as_deref()),
        ],
        "587",
    );
    Some(format!("{host}:{port}"))
}

/// Submission implicit TLS (`submission tls://…` / `__SUBMISSION_TLS_PORT__`).
pub fn effective_submission_tls_listen(config: &AppConfig, db: &DbMailPorts) -> Option<String> {
    let has_conf = config.submission_tls_listen.is_some();
    let has_db = db
        .submission_tls_port
        .as_deref()
        .is_some_and(|s| !s.is_empty());
    if !has_conf && !has_db {
        return None;
    }
    let host = listen_host(
        config
            .submission_tls_listen
            .as_deref()
            .or(config.submission_listen.as_deref()),
    );
    let port = first_port(
        [
            db.submission_tls_port.clone(),
            port_from_listen(config.submission_tls_listen.as_deref()),
        ],
        "465",
    );
    Some(format!("{host}:{port}"))
}

/// PEM paths for server TLS (`tls file` in maddy.conf, else `$(state_dir)/certs/`).
pub fn effective_tls_pem_paths(config: &AppConfig, state_dir: &Path) -> (PathBuf, PathBuf) {
    if let (Some(c), Some(k)) = (&config.tls_cert_path, &config.tls_key_path) {
        return (c.clone(), k.clone());
    }
    let dir = state_dir.join("certs");
    (dir.join("fullchain.pem"), dir.join("privkey.pem"))
}

/// Plain IMAP `host:port` when `imap tcp://…` or `__IMAP_PORT__` is set.
pub fn effective_imap_plain_listen(config: &AppConfig, db: &DbMailPorts) -> Option<String> {
    if let Ok(addr) = std::env::var("CHATMAIL_IMAP_ADDR") {
        if !addr.is_empty() {
            return Some(addr);
        }
    }
    let has_conf = config.imap_listen.is_some();
    let has_db = db.imap_port.as_deref().is_some_and(|s| !s.is_empty());
    if !has_conf && !has_db {
        if config.imap_tls_listen.is_some()
            || db.imap_tls_port.as_deref().is_some_and(|s| !s.is_empty())
        {
            return None;
        }
        let host = listen_host(None);
        return Some(format!("{host}:143"));
    }
    let host = listen_host(
        config
            .imap_listen
            .as_deref()
            .or(config.imap_tls_listen.as_deref()),
    );
    let port = first_port(
        [
            db.imap_port.clone(),
            port_from_listen(config.imap_listen.as_deref()),
        ],
        "143",
    );
    Some(format!("{host}:{port}"))
}

/// TLS IMAP `host:port` when `imap tls://…` or `__IMAP_TLS_PORT__` is set.
pub fn effective_imap_tls_listen(config: &AppConfig, db: &DbMailPorts) -> Option<String> {
    if std::env::var("CHATMAIL_IMAP_ADDR")
        .ok()
        .is_some_and(|s| !s.is_empty())
    {
        return None;
    }
    let has_conf = config.imap_tls_listen.is_some();
    let has_db = db.imap_tls_port.as_deref().is_some_and(|s| !s.is_empty());
    if !has_conf && !has_db {
        return None;
    }
    let host = listen_host(
        config
            .imap_tls_listen
            .as_deref()
            .or(config.imap_listen.as_deref()),
    );
    let port = first_port(
        [
            db.imap_tls_port.clone(),
            port_from_listen(config.imap_tls_listen.as_deref()),
        ],
        "993",
    );
    Some(format!("{host}:{port}"))
}

/// Effective `host:port` for a single plain IMAP listener (dev / env override).
pub fn effective_imap_listen(config: &AppConfig, db: &DbMailPorts) -> String {
    effective_imap_plain_listen(config, db)
        .or_else(|| effective_imap_tls_listen(config, db))
        .unwrap_or_else(|| format!("{}:143", listen_host(None)))
}

/// Plain HTTP (`chatmail tcp://…` / `__HTTP_PORT__`).
pub fn effective_http_plain_listen(config: &AppConfig, db: &DbMailPorts) -> Option<String> {
    if let Ok(addr) = std::env::var("CHATMAIL_HTTP_ADDR") {
        if !addr.is_empty() {
            return Some(addr);
        }
    }
    let has_conf = config.http_listen.is_some();
    let has_db = db.http_port.as_deref().is_some_and(|s| !s.is_empty());
    if !has_conf && !has_db {
        if config.http_tls_listen.is_some()
            || db.https_port.as_deref().is_some_and(|s| !s.is_empty())
        {
            return None;
        }
        let host = listen_host(None);
        return Some(format!("{host}:8080"));
    }
    let host = listen_host(
        config
            .http_listen
            .as_deref()
            .or(config.http_tls_listen.as_deref()),
    );
    let port = first_port(
        [
            db.http_port.clone(),
            port_from_listen(config.http_listen.as_deref()),
        ],
        "80",
    );
    Some(format!("{host}:{port}"))
}

/// HTTPS (`chatmail tls://…` / `__HTTPS_PORT__`).
pub fn effective_http_tls_listen(config: &AppConfig, db: &DbMailPorts) -> Option<String> {
    if std::env::var("CHATMAIL_HTTP_ADDR")
        .ok()
        .is_some_and(|s| !s.is_empty())
    {
        return None;
    }
    let has_conf = config.http_tls_listen.is_some();
    let has_db = db.https_port.as_deref().is_some_and(|s| !s.is_empty());
    if !has_conf && !has_db {
        return None;
    }
    let host = listen_host(
        config
            .http_tls_listen
            .as_deref()
            .or(config.http_listen.as_deref()),
    );
    let port = first_port(
        [
            db.https_port.clone(),
            port_from_listen(config.http_tls_listen.as_deref()),
        ],
        "443",
    );
    Some(format!("{host}:{port}"))
}

/// Effective `host:port` for a single plain HTTP listener (dev / env override).
pub fn effective_http_listen(config: &AppConfig, db: &DbMailPorts) -> String {
    effective_http_plain_listen(config, db)
        .or_else(|| effective_http_tls_listen(config, db))
        .unwrap_or_else(|| format!("{}:8080", listen_host(None)))
}

fn resolve_port(
    db: Option<&str>,
    listen: Option<&str>,
    env_listen: Option<&str>,
    default: &str,
) -> (String, bool) {
    if let Some(p) = non_empty(db) {
        return (p.to_string(), true);
    }
    if let Some(p) = port_from_listen(listen) {
        return (p, true);
    }
    if let Some(p) = port_from_listen(env_listen) {
        return (p, true);
    }
    (default.to_string(), false)
}

fn listen_host(listen: Option<&str>) -> &str {
    listen
        .and_then(|a| a.rsplit_once(':'))
        .map(|(h, _)| h)
        .unwrap_or("0.0.0.0")
}

fn non_empty(s: Option<&str>) -> Option<&str> {
    s.filter(|v| !v.trim().is_empty())
}

/// Ports for dclogin from addresses the supervisor actually bound (not maddy.conf defaults).
fn runtime_mail_ports(
    tls_addr: Option<&str>,
    plain_addr: Option<&str>,
    config_tls: &str,
    config_plain: &str,
) -> (String, String) {
    let tls = port_from_listen(tls_addr).unwrap_or_else(|| config_tls.to_string());
    let plain = port_from_listen(plain_addr).unwrap_or_else(|| config_plain.to_string());
    (tls, plain)
}

/// Delta Chat `is` / `ss` when admin has not set `__DCLOGIN_*_SECURITY__`.
///
/// Madmail defaults to **ssl** (implicit TLS on 993/465), not `default` (which would
/// allow plain fallback on 143/587). Plain is only advertised when no TLS listener exists.
fn dclogin_ports(
    has_tls: bool,
    _has_plain: bool,
    tls_port: &str,
    plain_port: &str,
) -> (String, String, String) {
    if has_tls {
        return ("ssl".into(), tls_port.to_string(), plain_port.to_string());
    }
    let port = plain_port.to_string();
    ("plain".into(), port.clone(), port)
}

/// Host for IMAP/SMTP in dclogin (`ih` / `sh`).
pub fn client_connect_host(config: &AppConfig, http_host: Option<&str>) -> String {
    if let Some(p) = config.primary_domain.as_deref().filter(|s| !s.is_empty()) {
        return clean_host(p);
    }
    if let Some(h) = http_host {
        let host = h.split(':').next().unwrap_or(h).trim();
        if !host.is_empty() && !is_loopback(host) {
            return clean_host(host);
        }
    }
    if let Some(ip) = config.public_ip.as_deref() {
        let ip = clean_host(ip);
        if !ip.is_empty() && !is_loopback(&ip) {
            return ip;
        }
    }
    for h in [
        config.mx_domain.as_deref(),
        config.mail_domain.as_deref(),
        config.hostname.as_deref(),
        config.primary_domain.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        let h = clean_host(h);
        if !h.is_empty() && !is_loopback(&h) {
            return h;
        }
    }
    "127.0.0.1".into()
}

pub fn port_from_listen(addr: Option<&str>) -> Option<String> {
    let addr = addr?;
    let port = addr.rsplit_once(':')?.1;
    if port.chars().all(|c| c.is_ascii_digit()) {
        Some(port.to_string())
    } else {
        None
    }
}

/// True when the supervisor must load PEM material (implicit TLS and/or STARTTLS upgrade).
pub fn listeners_need_tls_cert(runtime: &RuntimeListeners) -> bool {
    runtime.imap_tls_addr.is_some()
        || runtime.submission_tls_addr.is_some()
        || runtime.http_tls_addr.is_some()
        || runtime.imap_plain_addr.is_some()
        || runtime.submission_plain_addr.is_some()
}

fn clean_host(s: &str) -> String {
    s.trim_matches(['[', ']']).to_string()
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
}

/// Map admin `dclogin_*_security` + ports to Delta Chat `is`/`ss` and port (www `dcloginEndpoint`).
fn dclogin_endpoint(security: &str, starttls_port: &str, tls_port: &str) -> (String, String) {
    match security {
        "plain" => ("plain".into(), starttls_port.to_string()),
        "starttls" => ("starttls".into(), starttls_port.to_string()),
        "default" => ("default".into(), tls_port.to_string()),
        _ => ("ssl".into(), tls_port.to_string()),
    }
}

/// Percent-encode for dclogin query `p=` (JavaScript `encodeURIComponent`).
fn encode_dclogin_password(password: &str) -> String {
    let mut out = String::with_capacity(password.len() * 3);
    for b in password.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// Full `dclogin:` setup URI (Madmail www `createDcloginLink` / `chatmail.go` redirect).
pub fn build_dclogin_link(email: &str, password: &str, mail: &DcloginMailSettings) -> String {
    let host = &mail.client_host;
    let (imap_is, imap_ip) = dclogin_endpoint(
        &mail.dclogin_imap_security,
        &mail.imap_port_starttls,
        &mail.imap_port_tls,
    );
    let (smtp_ss, smtp_sp) = dclogin_endpoint(
        &mail.dclogin_smtp_security,
        &mail.smtp_port_starttls,
        &mail.smtp_port_tls,
    );
    let p = encode_dclogin_password(password);
    format!(
        "dclogin:{email}/?p={p}&v=1&ih={host}&ip={imap_ip}&is={imap_is}&sh={host}&sp={smtp_sp}&ss={smtp_ss}&ic=3"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_dclogin_link_matches_www_shape() {
        let mail = DcloginMailSettings {
            client_host: "1.1.1.1".into(),
            imap_port_tls: "993".into(),
            imap_port_starttls: "143".into(),
            smtp_port_tls: "465".into(),
            smtp_port_starttls: "587".into(),
            dclogin_imap_security: "ssl".into(),
            dclogin_smtp_security: "ssl".into(),
        };
        let uri = build_dclogin_link("user@[1.1.1.1]", "p@ss:word", &mail);
        assert!(uri.starts_with("dclogin:user@[1.1.1.1]/?p="));
        assert!(uri.contains("&v=1&ih=1.1.1.1&ip=993&is=ssl&sh=1.1.1.1&sp=465&ss=ssl&ic=3"));
        assert!(uri.contains("p%40ss%3Aword"));
    }

    #[test]
    fn listeners_need_tls_cert_for_starttls_only_ports() {
        let starttls_only = RuntimeListeners {
            imap_plain_addr: Some("0.0.0.0:143".into()),
            imap_tls_addr: None,
            submission_plain_addr: Some("0.0.0.0:587".into()),
            submission_tls_addr: None,
            smtp_addr: Some("0.0.0.0:25".into()),
            http_plain_addr: Some("0.0.0.0:8080".into()),
            http_tls_addr: None,
        };
        assert!(listeners_need_tls_cert(&starttls_only));

        let smtp_only = RuntimeListeners {
            imap_plain_addr: None,
            imap_tls_addr: None,
            submission_plain_addr: None,
            submission_tls_addr: None,
            smtp_addr: Some("0.0.0.0:25".into()),
            http_plain_addr: Some("0.0.0.0:8080".into()),
            http_tls_addr: None,
        };
        assert!(!listeners_need_tls_cert(&smtp_only));
    }

    #[test]
    fn client_host_uses_primary_over_loopback_http() {
        let cfg = AppConfig {
            primary_domain: Some("127.0.0.1".into()),
            ..Default::default()
        };
        assert_eq!(
            client_connect_host(&cfg, Some("localhost:8080")),
            "127.0.0.1"
        );
    }

    #[test]
    fn plain_imap_dev_defaults() {
        let cfg = AppConfig {
            imap_listen: Some("0.0.0.0:1143".into()),
            submission_listen: Some("0.0.0.0:2525".into()),
            ..Default::default()
        };
        let s = DcloginMailSettings::from_config(&cfg, Some("192.168.1.10:8080"));
        assert_eq!(s.client_host, "192.168.1.10");
        assert_eq!(s.dclogin_imap_security, "plain");
        assert_eq!(s.imap_port_starttls, "1143");
        assert_eq!(s.dclogin_smtp_security, "plain");
        assert_eq!(s.smtp_port_starttls, "2525");
    }

    #[test]
    fn runtime_listeners_ssl_when_plain_and_tls_bound() {
        let cfg = AppConfig {
            imap_listen: Some("0.0.0.0:143".into()),
            imap_tls_listen: Some("0.0.0.0:993".into()),
            smtp_listen: Some("0.0.0.0:25".into()),
            submission_listen: Some("0.0.0.0:587".into()),
            submission_tls_listen: Some("0.0.0.0:465".into()),
            ..Default::default()
        };
        let rt = RuntimeListeners {
            imap_plain_addr: Some("0.0.0.0:143".into()),
            imap_tls_addr: Some("0.0.0.0:993".into()),
            submission_plain_addr: Some("0.0.0.0:587".into()),
            submission_tls_addr: Some("0.0.0.0:465".into()),
            smtp_addr: Some("0.0.0.0:25".into()),
            http_plain_addr: None,
            http_tls_addr: None,
        };
        let s = DcloginMailSettings::from_config_with_db_and_runtime(
            &cfg,
            None,
            &DbMailPorts::default(),
            Some(&rt),
        );
        assert_eq!(s.dclogin_imap_security, "ssl");
        assert_eq!(s.imap_port_tls, "993");
        assert_eq!(s.imap_port_starttls, "143");
        assert_eq!(s.dclogin_smtp_security, "ssl");
        assert_eq!(s.smtp_port_tls, "465");
        assert_eq!(s.smtp_port_starttls, "587");
    }

    #[test]
    fn runtime_uses_bound_imap_port_not_config_default_143() {
        let cfg = AppConfig {
            http_listen: Some("0.0.0.0:8080".into()),
            ..Default::default()
        };
        let rt = RuntimeListeners {
            imap_plain_addr: Some("0.0.0.0:1143".into()),
            imap_tls_addr: None,
            submission_plain_addr: None,
            submission_tls_addr: None,
            smtp_addr: Some("0.0.0.0:25".into()),
            http_plain_addr: Some("0.0.0.0:8080".into()),
            http_tls_addr: None,
        };
        let s = DcloginMailSettings::from_config_with_db_and_runtime(
            &cfg,
            Some("127.0.0.1:8080"),
            &DbMailPorts::default(),
            Some(&rt),
        );
        assert_eq!(s.dclogin_imap_security, "plain");
        assert_eq!(s.imap_port_starttls, "1143");
        assert_eq!(s.imap_port_tls, "1143");
    }

    #[test]
    fn effective_imap_listeners_from_maddy_conf() {
        let cfg = AppConfig {
            imap_listen: Some("0.0.0.0:143".into()),
            imap_tls_listen: Some("0.0.0.0:993".into()),
            ..Default::default()
        };
        assert_eq!(
            effective_imap_plain_listen(&cfg, &DbMailPorts::default()).as_deref(),
            Some("0.0.0.0:143")
        );
        assert_eq!(
            effective_imap_tls_listen(&cfg, &DbMailPorts::default()).as_deref(),
            Some("0.0.0.0:993")
        );
    }

    #[test]
    fn tls_and_plain_imap_uses_ssl_mode() {
        let cfg = AppConfig {
            imap_listen: Some("0.0.0.0:143".into()),
            imap_tls_listen: Some("0.0.0.0:993".into()),
            submission_listen: Some("0.0.0.0:587".into()),
            submission_tls_listen: Some("0.0.0.0:465".into()),
            mail_domain: Some("mail.example.org".into()),
            ..Default::default()
        };
        let s = DcloginMailSettings::from_config(&cfg, None);
        assert_eq!(s.dclogin_imap_security, "ssl");
        assert_eq!(s.dclogin_smtp_security, "ssl");
        assert_eq!(s.imap_port_tls, "993");
        assert_eq!(s.imap_port_starttls, "143");
        assert_eq!(s.client_host, "mail.example.org");
    }

    #[test]
    fn db_smtp_port_overrides_maddy_conf_port_25() {
        let cfg = AppConfig {
            smtp_listen: Some("0.0.0.0:25".into()),
            ..Default::default()
        };
        let db = DbMailPorts {
            smtp_port: Some("2525".into()),
            ..Default::default()
        };
        assert_eq!(effective_smtp_listen(&cfg, &db), "0.0.0.0:2525");
    }

    #[test]
    fn db_submission_port_overrides_config_for_dclogin() {
        let cfg = AppConfig {
            submission_listen: Some("0.0.0.0:2525".into()),
            ..Default::default()
        };
        let db = DbMailPorts {
            submission_port: Some("2587".into()),
            submission_tls_port: Some("2465".into()),
            ..Default::default()
        };
        let s = DcloginMailSettings::from_config_with_db(&cfg, None, &db);
        assert_eq!(s.smtp_port_starttls, "2587");
        assert_eq!(s.smtp_port_tls, "2465");
        assert_eq!(
            effective_submission_plain_listen(&cfg, &db).as_deref(),
            Some("0.0.0.0:2587")
        );
        assert_eq!(effective_smtp_listen(&cfg, &db), "0.0.0.0:25");
    }

    #[test]
    fn db_smtp_port_used_when_no_submission_port() {
        let cfg = AppConfig {
            smtp_listen: Some("0.0.0.0:2525".into()),
            ..Default::default()
        };
        let db = DbMailPorts {
            smtp_port: Some("9025".into()),
            ..Default::default()
        };
        let s = DcloginMailSettings::from_config_with_db(&cfg, None, &db);
        assert_eq!(s.smtp_port_starttls, "9025");
        assert_eq!(effective_smtp_listen(&cfg, &db), "0.0.0.0:9025");
    }

    #[test]
    fn db_dclogin_security_override() {
        let cfg = AppConfig::default();
        let db = DbMailPorts {
            dclogin_smtp_security: Some("plain".into()),
            submission_port: Some("2525".into()),
            ..Default::default()
        };
        let s = DcloginMailSettings::from_config_with_db(&cfg, None, &db);
        assert_eq!(s.dclogin_smtp_security, "plain");
    }
}
