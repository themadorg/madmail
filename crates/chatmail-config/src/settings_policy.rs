// Copyright (C) 2026 themadorg
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared deployment settings registry. Values are never included in diagnostics.
use chatmail_types::{ChatmailError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedValue {
    pub value: String,
    pub source: String,
}
impl std::fmt::Debug for ProtectedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtectedValue")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsPolicy(pub BTreeMap<String, ProtectedValue>);

pub const SETTINGS: &[(&str, &str)] = &[
    ("registration_open", "__REGISTRATION_OPEN__"),
    ("jit_registration_enabled", "__JIT_REGISTRATION_ENABLED__"),
    (
        "registration_token_required",
        "__REGISTRATION_TOKEN_REQUIRED__",
    ),
    ("turn_enabled", "__TURN_ENABLED__"),
    ("iroh_enabled", "__IROH_ENABLED__"),
    ("ss_enabled", "__SS_ENABLED__"),
    ("auto_purge_seen", "__AUTO_PURGE_SEEN__"),
    ("message_retention_enabled", "__MESSAGE_RETENTION_ENABLED__"),
    ("message_retention", "__MESSAGE_RETENTION__"),
    ("admin_web_enabled", "__ADMIN_WEB_ENABLED__"),
    ("webimap_enabled", "__WEBIMAP_ENABLED__"),
    ("websmtp_enabled", "__WEBSMTP_ENABLED__"),
    ("webmail_cors_origins", "__WEBMAIL_CORS_ORIGINS__"),
    ("push_enabled", "__PUSH_ENABLED__"),
    ("push_mode", "__PUSH_MODE__"),
    ("federation_policy", "__FEDERATION_POLICY__"),
    ("federation_enabled", "__FEDERATION_ENABLED__"),
    ("allow_inbound_remote_rcpt", "__ALLOW_INBOUND_REMOTE_RCPT__"),
    ("shared_port_imap", "__SHARED_PORT_IMAP__"),
    ("shared_port_smtp", "__SHARED_PORT_SMTP__"),
    ("smtp_port", "__SMTP_PORT__"),
    ("submission_port", "__SUBMISSION_PORT__"),
    ("submission_tls_port", "__SUBMISSION_TLS_PORT__"),
    ("imap_port", "__IMAP_PORT__"),
    ("imap_tls_port", "__IMAP_TLS_PORT__"),
    ("turn_port", "__TURN_PORT__"),
    ("sasl_port", "__SASL_PORT__"),
    ("iroh_port", "__IROH_PORT__"),
    ("ss_port", "__SS_PORT__"),
    ("http_port", "__HTTP_PORT__"),
    ("https_port", "__HTTPS_PORT__"),
    ("http_enabled", "__HTTP_ENABLED__"),
    ("https_enabled", "__HTTPS_ENABLED__"),
    ("smtp_local_only", "__SMTP_LOCAL_ONLY__"),
    ("submission_local_only", "__SUBMISSION_LOCAL_ONLY__"),
    ("submission_tls_local_only", "__SUBMISSION_TLS_LOCAL_ONLY__"),
    ("imap_local_only", "__IMAP_LOCAL_ONLY__"),
    ("imap_tls_local_only", "__IMAP_TLS_LOCAL_ONLY__"),
    ("turn_local_only", "__TURN_LOCAL_ONLY__"),
    ("sasl_local_only", "__SASL_LOCAL_ONLY__"),
    ("iroh_local_only", "__IROH_LOCAL_ONLY__"),
    ("http_local_only", "__HTTP_LOCAL_ONLY__"),
    ("https_local_only", "__HTTPS_LOCAL_ONLY__"),
    ("smtp_hostname", "__SMTP_HOSTNAME__"),
    ("turn_realm", "__TURN_REALM__"),
    ("turn_secret", "__TURN_SECRET__"),
    ("turn_relay_ip", "__TURN_RELAY_IP__"),
    ("turn_relay_port_min", "__TURN_RELAY_PORT_MIN__"),
    ("turn_relay_port_max", "__TURN_RELAY_PORT_MAX__"),
    ("turn_ttl", "__TURN_TTL__"),
    ("iroh_relay_url", "__IROH_RELAY_URL__"),
    ("ss_cipher", "__SS_CIPHER__"),
    ("ss_password", "__SS_PASSWORD__"),
    ("admin_path", "__ADMIN_PATH__"),
    ("admin_web_path", "__ADMIN_WEB_PATH__"),
    ("dclogin_imap_security", "__DCLOGIN_IMAP_SECURITY__"),
    ("dclogin_smtp_security", "__DCLOGIN_SMTP_SECURITY__"),
    ("language", "__LANGUAGE__"),
    ("appendlimit", "__APPENDLIMIT__"),
    ("max_message_size", "__MAX_MESSAGE_SIZE__"),
    ("max_federation_size", "__MAX_FEDERATION_SIZE__"),
];

pub fn setting_key(name: &str) -> Option<&'static str> {
    SETTINGS.iter().find(|(n, _)| *n == name).map(|(_, k)| *k)
}
pub fn setting_name(key: &str) -> &str {
    SETTINGS
        .iter()
        .find(|(_, k)| *k == key)
        .map(|(n, _)| *n)
        .unwrap_or(key)
}
pub fn secret_setting(key: &str) -> bool {
    key.contains("PASSWORD") || key.contains("SECRET")
}
impl SettingsPolicy {
    pub fn insert(&mut self, name: &str, value: &str, source: impl Into<String>) -> Result<()> {
        let key = setting_key(name).ok_or_else(|| {
            ChatmailError::config(format!(
                "unknown setting {name}; see configuration reference"
            ))
        })?;
        let value = validate(name, value)?;
        self.0.insert(
            key.into(),
            ProtectedValue {
                value,
                source: source.into(),
            },
        );
        Ok(())
    }
    pub fn apply_environment(&mut self, mut get: impl FnMut(&str) -> Option<String>) -> Result<()> {
        for (name, _) in SETTINGS {
            let env = format!("CHATMAIL_{}", name.to_ascii_uppercase());
            if let Some(value) = get(&env) {
                self.insert(name, &value, format!("environment:{env}"))?;
            }
        }
        Ok(())
    }
    pub fn sources(&self) -> BTreeMap<String, String> {
        self.0
            .iter()
            .map(|(k, v)| (k.clone(), v.source.clone()))
            .collect()
    }
    pub fn check_writable(&self, keys: &[&str]) -> Result<()> {
        for key in keys {
            if let Some(value) = self.0.get(*key) {
                return Err(ChatmailError::ConfigurationLocked {
                    setting: setting_name(key).into(),
                    source_name: value.source.clone(),
                });
            }
        }
        Ok(())
    }
}

pub fn validate(name: &str, value: &str) -> Result<String> {
    let invalid = || ChatmailError::config(format!("invalid value for setting {name}"));
    let value = value.trim();
    if name.ends_with("_enabled")
        || name.ends_with("_local_only")
        || matches!(
            name,
            "registration_open"
                | "registration_token_required"
                | "allow_inbound_remote_rcpt"
                | "shared_port_imap"
                | "shared_port_smtp"
                | "auto_purge_seen"
        )
    {
        return match value.to_ascii_lowercase().as_str() {
            "true" | "yes" | "y" | "1" | "on" | "enable" | "enabled" | "t" => Ok("true".into()),
            "false" | "no" | "n" | "0" | "off" | "disable" | "disabled" | "f" => Ok("false".into()),
            _ => Err(invalid()),
        };
    }
    if name.ends_with("_port") || name.starts_with("turn_relay_port_") {
        let n = value.parse::<u16>().map_err(|_| invalid())?;
        if n == 0 {
            return Err(invalid());
        }
        return Ok(n.to_string());
    }
    if name == "turn_ttl" {
        let n = value.parse::<u64>().map_err(|_| invalid())?;
        if n == 0 {
            return Err(invalid());
        }
    }
    if name == "message_retention"
        && crate::parse_duration(value)
            .map_err(|_| invalid())?
            .is_zero()
    {
        return Err(invalid());
    }
    if matches!(
        name,
        "appendlimit" | "max_message_size" | "max_federation_size"
    ) {
        crate::parse_data_size(value).map_err(|_| invalid())?;
    }
    if matches!(name, "admin_path" | "admin_web_path") {
        let path = format!("/{}", value.trim_matches('/'));
        if path == "/"
            || path.contains(['?', '#', '{', '}', '*', '\\'])
            || path.chars().any(char::is_whitespace)
            || path
                .split('/')
                .any(|part| part == "." || part == ".." || part.contains('%'))
        {
            return Err(invalid());
        }
        return Ok(path);
    }
    if name == "turn_relay_ip" && value.parse::<std::net::IpAddr>().is_err() {
        return Err(invalid());
    }
    if name == "iroh_relay_url" {
        let uri: http::Uri = value.parse().map_err(|_| invalid())?;
        if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.host().is_none() {
            return Err(invalid());
        }
    }
    if name == "ss_cipher" {
        let normalized = value.to_ascii_lowercase().replace('_', "-");
        if !["aes-128-gcm", "aes-256-gcm", "chacha20-ietf-poly1305"].contains(&normalized.as_str())
        {
            return Err(invalid());
        }
        return Ok(normalized);
    }
    if name == "language" && !["en", "fa", "ru", "es"].contains(&value) {
        return Err(invalid());
    }
    if name == "push_mode" && !["auto", "on", "off"].contains(&value) {
        return Err(invalid());
    }
    if name == "federation_policy" && !["ACCEPT", "REJECT"].contains(&value) {
        return Err(invalid());
    }
    if name.ends_with("_security") && !["ssl", "starttls", "plain", "default"].contains(&value) {
        return Err(invalid());
    }
    if value.is_empty() && !matches!(name, "webmail_cors_origins" | "ss_password" | "turn_secret") {
        return Err(invalid());
    }
    Ok(value.into())
}

impl crate::AppConfig {
    /// Include legacy module directives when callers construct AppConfig directly.
    pub fn deployment_policy(&self) -> Result<SettingsPolicy> {
        let mut policy = SettingsPolicy::default();
        macro_rules! optional {
            ($($name:literal => $field:ident),* $(,)?) => { $(
                if let Some(v) = &self.$field { policy.insert($name, v, "file")?; }
            )* };
        }
        optional!("smtp_hostname" => hostname, "admin_path" => admin_path,
            "admin_web_path" => admin_web_path, "language" => language,
            "turn_realm" => turn_realm, "turn_secret" => turn_secret,
            "turn_relay_ip" => turn_relay_ip, "iroh_relay_url" => iroh_relay_url,
            "ss_password" => ss_password, "ss_cipher" => ss_cipher,
            "appendlimit" => appendlimit, "max_message_size" => max_message_size,
            "max_federation_size" => max_federation_size, "message_retention" => retention);
        for (name, value) in [
            ("turn_port", u64::from(self.turn_port)),
            ("turn_ttl", self.turn_ttl),
            ("turn_relay_port_min", u64::from(self.turn_relay_port_min)),
            ("turn_relay_port_max", u64::from(self.turn_relay_port_max)),
            ("iroh_port", u64::from(self.iroh_port)),
        ] {
            if value != 0 {
                policy.insert(name, &value.to_string(), "file")?;
            }
        }
        for (name, listen) in [
            ("smtp", &self.smtp_listen),
            ("submission", &self.submission_listen),
            ("submission_tls", &self.submission_tls_listen),
            ("imap", &self.imap_listen),
            ("imap_tls", &self.imap_tls_listen),
            ("http", &self.http_listen),
            ("https", &self.http_tls_listen),
            ("turn", &self.turn_listen_udp),
            ("ss", &self.ss_addr),
        ] {
            if let Some(listen) = listen {
                if let Some(port) = crate::port_from_listen(Some(listen)) {
                    policy.insert(&format!("{name}_port"), &port.to_string(), "file")?;
                }
                let local_name = format!("{name}_local_only");
                if setting_key(&local_name).is_some() {
                    let host = listen
                        .rsplit_once(':')
                        .map(|(h, _)| h.trim_matches(['[', ']']))
                        .unwrap_or("");
                    policy.insert(
                        &local_name,
                        if host == "localhost"
                            || host
                                .parse::<std::net::IpAddr>()
                                .is_ok_and(|ip| ip.is_loopback())
                        {
                            "true"
                        } else {
                            "false"
                        },
                        "file",
                    )?;
                }
            }
        }
        for (name, value) in [
            ("shared_port_imap", &self.alpn_imap),
            ("shared_port_smtp", &self.alpn_smtp),
        ] {
            if value.is_some() {
                policy.insert(name, "true", "file")?;
            }
        }
        policy.0.extend(self.settings_policy.0.clone());
        Ok(policy)
    }

    pub fn validate_deployment(&self) -> Result<()> {
        let policy = self.deployment_policy()?;
        let get = |name: &str| {
            setting_key(name)
                .and_then(|k| policy.0.get(k))
                .map(|v| v.value.as_str())
        };
        let min = get("turn_relay_port_min")
            .unwrap_or("49152")
            .parse::<u16>()
            .unwrap_or(49152);
        let max = get("turn_relay_port_max")
            .unwrap_or("65535")
            .parse::<u16>()
            .unwrap_or(65535);
        let control = get("turn_port")
            .unwrap_or("3478")
            .parse::<u16>()
            .unwrap_or(3478);
        crate::turn_relay_ports::TurnRelayPortRange { min, max }
            .validate(control)
            .map_err(ChatmailError::config)?;
        let api = get("admin_path").unwrap_or("/api/admin");
        let web = get("admin_web_path").unwrap_or("/admin");
        if api == web || api.starts_with(&format!("{web}/")) || web.starts_with(&format!("{api}/"))
        {
            return Err(ChatmailError::config(
                "admin_path and admin_web_path must not overlap",
            ));
        }
        Ok(())
    }
}

impl crate::AppConfig {
    pub fn apply_deployment_environment(&mut self) -> Result<()> {
        self.settings_policy
            .apply_environment(|key| std::env::var(key).ok())?;
        for (env, service, field) in [
            ("CHATMAIL_SMTP_ADDR", "smtp", &mut self.smtp_listen),
            ("CHATMAIL_IMAP_ADDR", "imap", &mut self.imap_listen),
            ("CHATMAIL_HTTP_ADDR", "http", &mut self.http_listen),
        ] {
            if let Ok(addr) = std::env::var(env) {
                let parsed: std::net::SocketAddr = addr.parse().map_err(|_| {
                    ChatmailError::config(format!("invalid listen address in {env}"))
                })?;
                self.settings_policy.insert(
                    &format!("{service}_port"),
                    &parsed.port().to_string(),
                    format!("environment:{env}"),
                )?;
                self.settings_policy.insert(
                    &format!("{service}_local_only"),
                    &parsed.ip().is_loopback().to_string(),
                    format!("environment:{env}"),
                )?;
                *field = Some(addr);
            }
        }
        self.apply_policy_fields();
        Ok(())
    }
}

impl crate::AppConfig {
    /// Keep legacy static consumers consistent with the canonical deployment registry.
    pub fn apply_policy_fields(&mut self) {
        macro_rules! text { ($($name:literal => $field:ident),* $(,)?) => { $(
            if let Some(v) = setting_key($name).and_then(|k| self.settings_policy.0.get(k)) { self.$field = Some(v.value.clone()); }
        )* }; }
        text!("smtp_hostname" => hostname, "admin_path" => admin_path, "admin_web_path" => admin_web_path,
            "language" => language, "turn_realm" => turn_realm, "turn_secret" => turn_secret,
            "turn_relay_ip" => turn_relay_ip, "iroh_relay_url" => iroh_relay_url, "ss_password" => ss_password,
            "ss_cipher" => ss_cipher, "appendlimit" => appendlimit, "max_message_size" => max_message_size,
            "max_federation_size" => max_federation_size, "message_retention" => retention);
        macro_rules! number { ($($name:literal => $field:ident),* $(,)?) => { $(
            if let Some(v) = setting_key($name).and_then(|k| self.settings_policy.0.get(k)) { if let Ok(n) = v.value.parse() { self.$field = n; } }
        )* }; }
        number!("turn_port" => turn_port, "turn_ttl" => turn_ttl, "turn_relay_port_min" => turn_relay_port_min,
            "turn_relay_port_max" => turn_relay_port_max, "iroh_port" => iroh_port);
        for (name, field) in [
            ("turn_enabled", &mut self.turn_enable),
            ("iroh_enabled", &mut self.iroh_enable),
            (
                "allow_inbound_remote_rcpt",
                &mut self.allow_inbound_remote_rcpt,
            ),
        ] {
            if let Some(v) = setting_key(name).and_then(|k| self.settings_policy.0.get(k)) {
                *field = crate::parse_bool_str(&v.value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_wins_and_false_is_explicit() {
        let mut p = SettingsPolicy::default();
        p.insert("admin_web_enabled", "true", "file:test.toml")
            .unwrap();
        p.apply_environment(|key| (key == "CHATMAIL_ADMIN_WEB_ENABLED").then(|| "false".into()))
            .unwrap();
        let v = &p.0["__ADMIN_WEB_ENABLED__"];
        assert_eq!(v.value, "false");
        assert_eq!(v.source, "environment:CHATMAIL_ADMIN_WEB_ENABLED");
        assert!(p.check_writable(&["__ADMIN_WEB_ENABLED__"]).is_err());
        assert!(p.check_writable(&["__ADMIN_WEB_PATH__"]).is_ok());
    }

    #[test]
    fn registry_names_and_keys_are_unique() {
        let mut names = std::collections::BTreeSet::new();
        let mut keys = std::collections::BTreeSet::new();
        for (name, key) in SETTINGS {
            assert!(names.insert(name));
            assert!(keys.insert(key));
            assert_eq!(setting_key(name), Some(*key));
        }
    }

    #[test]
    fn rejects_invalid_inputs_without_echoing_values() {
        for (name, value) in [
            ("admin_web_enabled", "secret-typo"),
            ("turn_relay_port_min", "0"),
            ("turn_relay_port_max", "65536"),
            ("message_retention", "14x"),
            ("message_retention", "0d"),
            ("admin_path", "/"),
            ("admin_path", "/api/{secret}"),
            ("admin_path", "/api/../admin"),
            ("admin_web_path", "/admin?token=secret"),
            ("push_mode", "maybe"),
        ] {
            let error = validate(name, value).unwrap_err().to_string();
            assert!(!error.contains("secret"));
        }
    }

    #[test]
    fn policy_debug_and_sources_never_contain_secret_values() {
        let mut p = SettingsPolicy::default();
        p.insert(
            "turn_secret",
            "very-private-secret",
            "environment:CHATMAIL_TURN_SECRET",
        )
        .unwrap();
        assert!(!format!("{p:?}").contains("very-private-secret"));
        assert!(!format!("{:?}", p.sources()).contains("very-private-secret"));
    }

    #[test]
    fn overlapping_paths_rejected() {
        let mut cfg = crate::AppConfig::default();
        cfg.settings_policy
            .insert("admin_path", "/admin/api", "file")
            .unwrap();
        assert!(cfg.validate_deployment().is_err());
    }
}
