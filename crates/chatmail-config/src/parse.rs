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

use std::path::{Path, PathBuf};

use chatmail_types::{ChatmailError, Result};
use serde::Deserialize;

use crate::maddy;
use crate::AppConfig;

/// TOML configuration file (`chatmail.toml`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct TomlConfig {
    pub hostname: Option<String>,
    pub primary_domain: Option<String>,
    pub local_domains: Option<String>,
    pub public_ip: Option<String>,
    pub state_dir: Option<String>,
    pub runtime_dir: Option<String>,
    pub tls_mode: Option<String>,
    pub acme_email: Option<String>,
    pub debug: Option<bool>,
    pub log: Option<String>,
    pub smtp_listen: Option<String>,
    pub submission_listen: Option<String>,
    pub submission_tls_listen: Option<String>,
    pub imap_listen: Option<String>,
    pub imap_tls_listen: Option<String>,
    pub http_listen: Option<String>,
    pub http_tls_listen: Option<String>,
    pub openmetrics_listen: Option<String>,
    pub turn_enable: Option<bool>,
    pub turn_server: Option<String>,
    pub turn_port: Option<u16>,
    pub turn_secret: Option<String>,
    pub turn_ttl: Option<u64>,
    pub turn_listen_udp: Option<String>,
    pub turn_realm: Option<String>,
    pub admin_web_path: Option<String>,
    pub language: Option<String>,
    pub www_dir: Option<String>,
    pub auth_auto_create: Option<bool>,
    pub jit_domain: Option<String>,
}

/// Load static configuration from `chatmail.toml` or Madmail-style `*.conf`.
pub fn load_config(path: &Path) -> Result<AppConfig> {
    let content = std::fs::read_to_string(path).map_err(ChatmailError::from)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "toml" => Ok(toml_to_app_config(&content)?),
        "conf" => {
            maddy::parse_maddy_config(&content).map_err(|e| ChatmailError::config(e.to_string()))
        }
        _ => {
            if content.trim_start().starts_with('{') {
                Ok(toml_to_app_config(&content)?)
            } else {
                maddy::parse_maddy_config(&content)
                    .map_err(|e| ChatmailError::config(e.to_string()))
            }
        }
    }
}

fn toml_to_app_config(content: &str) -> Result<AppConfig> {
    let parsed: TomlConfig =
        toml::from_str(content).map_err(|e| ChatmailError::config(e.to_string()))?;
    // Do not copy primary_domain → mail_domain: registration domain comes from HTTP Host.
    let mx_domain = parsed.hostname.clone();
    Ok(AppConfig {
        hostname: parsed.hostname,
        primary_domain: parsed.primary_domain,
        local_domains: parsed.local_domains,
        public_ip: parsed.public_ip,
        state_dir: parsed.state_dir.map(Into::into),
        runtime_dir: parsed.runtime_dir.map(Into::into),
        tls_mode: parsed.tls_mode,
        acme_email: parsed.acme_email,
        tls_cert_path: None,
        tls_key_path: None,
        debug: parsed.debug.unwrap_or(false),
        log_target: parsed.log,
        auth_auto_create: parsed.auth_auto_create.unwrap_or(false),
        jit_domain: parsed.jit_domain,
        credentials_driver: None,
        credentials_dsn: None,
        imapsql_driver: None,
        imapsql_dsn: None,
        default_quota: None,
        retention: None,
        unused_account_retention: None,
        appendlimit: None,
        max_message_size: None,
        max_federation_size: None,
        mail_fsync: None,
        blob_dedup: None,
        mail_domain: None,
        mx_domain,
        username_length: None,
        password_length: None,
        min_username_length: None,
        max_username_length: None,
        password_min_length: None,
        admin_path: None,
        admin_web_path: parsed.admin_web_path,
        language: parsed.language,
        www_dir: parsed.www_dir.map(PathBuf::from),
        enable_contact_sharing: false,
        sharing_dsn: None,
        admin_token: None,
        smtp_listen: parsed.smtp_listen,
        submission_listen: parsed.submission_listen,
        submission_tls_listen: parsed.submission_tls_listen,
        imap_listen: parsed.imap_listen,
        imap_tls_listen: parsed.imap_tls_listen,
        http_listen: parsed.http_listen,
        http_tls_listen: parsed.http_tls_listen,
        openmetrics_listen: parsed.openmetrics_listen,
        queue: crate::QueueSettings::default(),
        turn_enable: parsed.turn_enable.unwrap_or(false),
        turn_server: parsed.turn_server,
        turn_port: parsed.turn_port.unwrap_or(0),
        turn_secret: parsed.turn_secret,
        turn_ttl: parsed.turn_ttl.unwrap_or(0),
        turn_listen_udp: parsed.turn_listen_udp,
        turn_listen_tcp: None,
        turn_realm: parsed.turn_realm,
        turn_relay_ip: None,
        turn_relay_port_min: 0,
        turn_relay_port_max: 0,
        turn_debug: false,
        turn_test_force_relay: false,
        iroh_relay_url: None,
        iroh_enable: false,
        iroh_port: 0,
        ss_addr: None,
        ss_password: None,
        ss_cipher: None,
        ss_cert_path: None,
        ss_key_path: None,
        ss_allowed_ports: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P1-UT02: TOML config loads `primary_domain` and related fields.
    #[test]
    fn p1_ut02_load_config_valid_toml() {
        let content = r#"
hostname = "mail.example.org"
primary_domain = "example.org"
state_dir = "/var/lib/chatmail"
tls_mode = "autocert"
"#;
        let cfg = toml_to_app_config(content).expect("toml");
        assert_eq!(cfg.hostname.as_deref(), Some("mail.example.org"));
        assert_eq!(cfg.primary_domain.as_deref(), Some("example.org"));
        assert_eq!(
            cfg.state_dir.as_deref(),
            Some(std::path::Path::new("/var/lib/chatmail"))
        );
        assert_eq!(cfg.tls_mode.as_deref(), Some("autocert"));
    }

    #[test]
    fn p1_ut02_load_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chatmail.toml");
        std::fs::write(&path, r#"primary_domain = "test.example.org""#).unwrap();
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.primary_domain.as_deref(), Some("test.example.org"));
    }

    #[test]
    fn p1_ut02_invalid_toml_returns_config_error() {
        let err = toml_to_app_config("not = [valid").unwrap_err();
        assert!(matches!(err, ChatmailError::Config(_)));
    }

    #[test]
    fn p1_ut02_invalid_maddy_conf_returns_config_error() {
        let err = maddy::parse_maddy_config("1bad whatever").unwrap_err();
        assert!(err.message.contains("directive name"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.conf");
        std::fs::write(&path, "1bad whatever\n").unwrap();
        let err = load_config(&path).unwrap_err();
        assert!(matches!(err, ChatmailError::Config(_)));
    }

    #[test]
    fn p1_ut02_load_maddy_conf_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("maddy.conf");
        std::fs::write(
            &path,
            "$(primary_domain) = example.org\nstate_dir /var/lib/maddy\n",
        )
        .unwrap();
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.primary_domain.as_deref(), Some("example.org"));
    }
}
