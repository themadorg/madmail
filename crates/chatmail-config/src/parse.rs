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
///
/// Bool fields accept TOML booleans and flexible strings/ints (`"yes"`, `"enable"`, `1`, …).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct TomlConfig {
    #[serde(flatten)]
    pub settings: std::collections::BTreeMap<String, toml::Value>,
    pub admin_token: Option<String>,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub credentials_driver: Option<String>,
    pub credentials_dsn: Option<String>,
    pub imapsql_driver: Option<String>,
    pub imapsql_dsn: Option<String>,
    pub default_quota: Option<String>,
    pub unused_account_retention: Option<String>,
    pub mail_fsync: Option<String>,
    pub blob_dedup: Option<String>,
    pub ss_addr: Option<String>,
    pub ss_cert_path: Option<String>,
    pub ss_key_path: Option<String>,
    pub turn_listen_tcp: Option<String>,
    pub turn_relay_ip: Option<String>,
    pub iroh_relay_url: Option<String>,
    pub ss_password: Option<String>,
    pub ss_cipher: Option<String>,
    pub admin_path: Option<String>,
    pub retention: Option<String>,
    pub appendlimit: Option<String>,
    pub max_message_size: Option<String>,
    pub max_federation_size: Option<String>,
    pub turn_relay_port_min: Option<u16>,
    pub turn_relay_port_max: Option<u16>,
    pub mail_domain: Option<String>,
    pub mx_domain: Option<String>,
    pub username_length: Option<u32>,
    pub password_length: Option<u32>,
    pub min_username_length: Option<u32>,
    pub max_username_length: Option<u32>,
    pub password_min_length: Option<u32>,
    pub sharing_dsn: Option<String>,
    pub iroh_port: Option<u16>,
    #[serde(default)]
    pub ss_allowed_ports: Vec<String>,
    #[serde(default)]
    pub queue: crate::QueueSettings,
    #[serde(
        default,
        deserialize_with = "crate::bool_str::deserialize_option_bool_flexible"
    )]
    pub enable_contact_sharing: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::bool_str::deserialize_option_bool_flexible"
    )]
    pub allow_inbound_remote_rcpt: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::bool_str::deserialize_option_bool_flexible"
    )]
    pub turn_debug: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::bool_str::deserialize_option_bool_flexible"
    )]
    pub turn_test_force_relay: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::bool_str::deserialize_option_bool_flexible"
    )]
    pub iroh_enable: Option<bool>,
    pub hostname: Option<String>,
    pub primary_domain: Option<String>,
    pub local_domains: Option<String>,
    pub public_ip: Option<String>,
    pub state_dir: Option<String>,
    pub runtime_dir: Option<String>,
    pub tls_mode: Option<String>,
    pub acme_email: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::bool_str::deserialize_option_bool_flexible"
    )]
    pub debug: Option<bool>,
    pub log: Option<String>,
    pub smtp_listen: Option<String>,
    pub submission_listen: Option<String>,
    pub submission_tls_listen: Option<String>,
    pub imap_listen: Option<String>,
    pub imap_tls_listen: Option<String>,
    pub http_listen: Option<String>,
    pub http_tls_listen: Option<String>,
    /// ALPN tokens that route the HTTPS TLS port to IMAP / submission.
    pub alpn_imap: Option<String>,
    pub alpn_smtp: Option<String>,
    /// Hostnames selecting mail on the shared TLS port when no ALPN is offered.
    pub sni_imap: Option<String>,
    pub sni_smtp: Option<String>,
    pub openmetrics_listen: Option<String>,
    pub openmetrics_username: Option<String>,
    pub openmetrics_password: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::bool_str::deserialize_option_bool_flexible"
    )]
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
    #[serde(
        default,
        deserialize_with = "crate::bool_str::deserialize_option_bool_flexible"
    )]
    pub websocket_bridge_enabled: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::bool_str::deserialize_option_bool_flexible"
    )]
    pub auth_auto_create: Option<bool>,
    pub jit_domain: Option<String>,
}

/// Load static configuration from `chatmail.toml` or Madmail-style `*.conf`.
pub fn load_config(path: &Path) -> Result<AppConfig> {
    let content = std::fs::read_to_string(path).map_err(ChatmailError::from)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let mut cfg = match ext {
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
    }?;
    cfg.settings_policy = cfg.deployment_policy()?;
    for value in cfg.settings_policy.0.values_mut() {
        value.source = format!("file:{}", path.display());
    }
    cfg.apply_deployment_environment()?;
    cfg.validate_deployment()?;
    Ok(cfg)
}

fn toml_to_app_config(content: &str) -> Result<AppConfig> {
    let parsed: TomlConfig =
        toml::from_str(content).map_err(|e| ChatmailError::config(e.to_string()))?;
    // Do not copy primary_domain → mail_domain: registration domain comes from HTTP Host.
    let mx_domain = parsed.hostname.clone();
    let mut settings_policy = crate::SettingsPolicy::default();
    let table: toml::Table =
        toml::from_str(content).map_err(|e| ChatmailError::config(e.to_string()))?;
    for (name, value) in &table {
        let name = match name.as_str() {
            "turn_enable" => "turn_enabled",
            "iroh_enable" => "iroh_enabled",
            "retention" => "message_retention",
            other => other,
        };
        if crate::settings_policy::setting_key(name).is_some() {
            let text = match value {
                toml::Value::String(v) => v.clone(),
                toml::Value::Boolean(v) => v.to_string(),
                toml::Value::Integer(v) => v.to_string(),
                _ => {
                    return Err(ChatmailError::config(format!(
                        "setting {name} requires a scalar value"
                    )))
                }
            };
            settings_policy.insert(name, &text, "file")?;
        }
    }
    for (name, value) in &parsed.settings {
        let text = match value {
            toml::Value::String(v) => v.clone(),
            toml::Value::Boolean(v) => v.to_string(),
            toml::Value::Integer(v) => v.to_string(),
            _ => {
                return Err(ChatmailError::config(format!(
                    "setting {name} requires a scalar value"
                )))
            }
        };
        settings_policy.insert(name, &text, "file")?;
    }
    if let Some(value) = parsed.turn_enable {
        settings_policy.insert("turn_enabled", &value.to_string(), "file")?;
    }
    let mut cfg = AppConfig {
        settings_policy,
        hostname: parsed.hostname,
        primary_domain: parsed.primary_domain,
        primary_domains: Vec::new(),
        local_domains: parsed.local_domains,
        public_ip: parsed.public_ip,
        state_dir: parsed.state_dir.map(Into::into),
        runtime_dir: parsed.runtime_dir.map(Into::into),
        tls_mode: parsed.tls_mode,
        acme_email: parsed.acme_email,
        tls_cert_path: parsed.tls_cert_path.map(Into::into),
        tls_key_path: parsed.tls_key_path.map(Into::into),
        debug: parsed.debug.unwrap_or(false),
        log_target: parsed.log,
        auth_auto_create: parsed.auth_auto_create.unwrap_or(false),
        jit_domain: parsed.jit_domain,
        credentials_driver: parsed.credentials_driver,
        credentials_dsn: parsed.credentials_dsn,
        imapsql_driver: parsed.imapsql_driver,
        imapsql_dsn: parsed.imapsql_dsn,
        default_quota: parsed.default_quota,
        retention: parsed.retention,
        unused_account_retention: parsed.unused_account_retention,
        appendlimit: parsed.appendlimit,
        max_message_size: parsed.max_message_size,
        max_federation_size: parsed.max_federation_size,
        mail_fsync: parsed.mail_fsync,
        blob_dedup: parsed.blob_dedup,
        mail_domain: parsed.mail_domain,
        mx_domain: parsed.mx_domain.or(mx_domain),
        username_length: parsed.username_length,
        password_length: parsed.password_length,
        min_username_length: parsed.min_username_length,
        max_username_length: parsed.max_username_length,
        password_min_length: parsed.password_min_length,
        admin_path: parsed.admin_path,
        admin_web_path: parsed.admin_web_path,
        language: parsed.language,
        www_dir: parsed.www_dir.map(PathBuf::from),
        enable_contact_sharing: parsed.enable_contact_sharing.unwrap_or(false),
        allow_inbound_remote_rcpt: parsed.allow_inbound_remote_rcpt.unwrap_or(false),
        sharing_dsn: parsed.sharing_dsn,
        admin_token: parsed.admin_token,
        websocket_bridge_enabled: parsed.websocket_bridge_enabled.unwrap_or(false),
        smtp_listen: parsed.smtp_listen,
        submission_listen: parsed.submission_listen,
        submission_tls_listen: parsed.submission_tls_listen,
        imap_listen: parsed.imap_listen,
        imap_tls_listen: parsed.imap_tls_listen,
        http_listen: parsed.http_listen,
        http_tls_listen: parsed.http_tls_listen,
        alpn_imap: parsed.alpn_imap,
        alpn_smtp: parsed.alpn_smtp,
        sni_imap: parsed.sni_imap,
        sni_smtp: parsed.sni_smtp,
        openmetrics_listen: parsed.openmetrics_listen,
        openmetrics_username: parsed.openmetrics_username,
        openmetrics_password: parsed.openmetrics_password,
        queue: parsed.queue,
        turn_enable: parsed.turn_enable.unwrap_or(false),
        turn_server: parsed.turn_server,
        turn_port: parsed.turn_port.unwrap_or(0),
        turn_secret: parsed.turn_secret,
        turn_ttl: parsed.turn_ttl.unwrap_or(0),
        turn_listen_udp: parsed.turn_listen_udp,
        turn_listen_tcp: parsed.turn_listen_tcp,
        turn_realm: parsed.turn_realm,
        turn_relay_ip: parsed.turn_relay_ip,
        turn_relay_port_min: parsed.turn_relay_port_min.unwrap_or(0),
        turn_relay_port_max: parsed.turn_relay_port_max.unwrap_or(0),
        turn_debug: parsed.turn_debug.unwrap_or(false),
        turn_test_force_relay: parsed.turn_test_force_relay.unwrap_or(false),
        iroh_relay_url: parsed.iroh_relay_url,
        iroh_enable: parsed.iroh_enable.unwrap_or(false),
        iroh_port: parsed.iroh_port.unwrap_or(0),
        ss_addr: parsed.ss_addr,
        ss_password: parsed.ss_password,
        ss_cipher: parsed.ss_cipher,
        ss_cert_path: parsed.ss_cert_path.map(Into::into),
        ss_key_path: parsed.ss_key_path.map(Into::into),
        ss_allowed_ports: parsed.ss_allowed_ports,
    };
    cfg.settings_policy = cfg.deployment_policy()?;
    cfg.validate_deployment()?;
    cfg.apply_policy_fields();
    cfg.normalize_primary_domains();
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_config_defaults_off_and_accepts_both_formats() {
        assert!(!toml_to_app_config("").unwrap().websocket_bridge_enabled);
        assert!(
            toml_to_app_config("websocket_bridge_enabled = true")
                .unwrap()
                .websocket_bridge_enabled
        );
        assert!(
            !toml_to_app_config("websocket_bridge_enabled = false")
                .unwrap()
                .websocket_bridge_enabled
        );
        assert!(
            crate::parse_maddy_config("chatmail {\n websocket_bridge_enabled true\n}\n")
                .unwrap()
                .websocket_bridge_enabled
        );
    }

    #[test]
    fn toml_primary_domain_list_accepts_commas() {
        let cfg = toml_to_app_config("primary_domain = \"b.com,c.com,1.1.1.1\"\n").unwrap();
        assert_eq!(cfg.primary_domain.as_deref(), Some("b.com"));
        assert_eq!(cfg.web_registration_domain(Some("c.com:443")), "c.com");
        assert_eq!(cfg.web_registration_domain(Some("1.1.1.1")), "[1.1.1.1]");
    }

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

    #[test]
    fn toml_debug_accepts_flexible_enable_disable() {
        for (src, want) in [
            (r#"debug = true"#, true),
            (r#"debug = "yes""#, true),
            (r#"debug = "True""#, true),
            (r#"debug = "1""#, true),
            (r#"debug = "enable""#, true),
            (r#"debug = 1"#, true),
            (r#"debug = false"#, false),
            (r#"debug = "no""#, false),
            (r#"debug = "disable""#, false),
            (r#"debug = 0"#, false),
        ] {
            let cfg = toml_to_app_config(src).unwrap_or_else(|e| panic!("{src}: {e}"));
            assert_eq!(cfg.debug, want, "src={src}");
        }
        // Omitted → disabled (No-Log friendly default)
        let cfg = toml_to_app_config(r#"hostname = "mail.example.org""#).unwrap();
        assert!(!cfg.debug);
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    #[test]
    fn toml_and_conf_operational_settings_have_equal_values() {
        let toml = toml_to_app_config(
            r#"
admin_web_enabled = false
admin_web_path = "/private-admin"
admin_path = "/private-api"
message_retention_enabled = true
message_retention = "14d"
turn_relay_port_min = 50000
turn_relay_port_max = 50100
"#,
        )
        .unwrap()
        .deployment_policy()
        .unwrap();
        let conf = maddy::parse_maddy_config("settings {\nadmin_web_enabled false\nadmin_web_path /private-admin\nadmin_path /private-api\nmessage_retention_enabled true\nmessage_retention 14d\nturn_relay_port_min 50000\nturn_relay_port_max 50100\n}\n").unwrap().deployment_policy().unwrap();
        assert_eq!(toml, conf);
    }
    #[test]
    fn unknown_toml_and_settings_block_keys_fail() {
        assert!(toml_to_app_config("admin_web_enabld = true").is_err());
        assert!(maddy::parse_maddy_config("settings {\nadmin_web_enabld true\n}\n").is_err());
    }
    #[test]
    fn absent_is_not_false_and_bad_range_fails() {
        assert!(!toml_to_app_config("")
            .unwrap()
            .deployment_policy()
            .unwrap()
            .0
            .contains_key("__ADMIN_WEB_ENABLED__"));
        assert!(
            toml_to_app_config("turn_relay_port_min = 51000\nturn_relay_port_max = 50000").is_err()
        );
    }
}
