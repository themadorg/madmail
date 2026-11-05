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
    /// Settings protected from database-backed CLI/API changes.
    ///
    /// Only explicit sources lock a setting: a `.conf` `settings { }` entry, a
    /// top-level TOML key named in [`SETTINGS`], or a `CHATMAIL_*` environment
    /// variable (including `CHATMAIL_{SMTP,IMAP,HTTP}_ADDR`). Legacy module
    /// directives (listener addresses, `alpn_*`, `hostname`, `turn_*`, `ss_*`,
    /// `iroh_relay_url`, `retention`, `appendlimit`, `max_message_size`,
    /// `language`, admin paths in `chatmail { }`) stay file defaults below the
    /// database, as before, so generated configurations never lock settings the
    /// operator did not choose.
    pub fn deployment_policy(&self) -> Result<SettingsPolicy> {
        Ok(self.settings_policy.clone())
    }

    /// Reject inconsistent protected values (combined with legacy module
    /// directives where they interact) before any listener starts.
    pub fn validate_deployment(&self) -> Result<()> {
        let policy = self.deployment_policy()?;
        let get = |name: &str| {
            setting_key(name)
                .and_then(|k| policy.0.get(k))
                .map(|v| v.value.clone())
        };
        let number = |name: &str, field: u16, default: u16| {
            get(name)
                .and_then(|v| v.parse::<u16>().ok())
                .or((field != 0).then_some(field))
                .unwrap_or(default)
        };
        // Legacy-only TURN values keep their previous runtime handling; once any of
        // them is protected, the effective combination must be valid.
        if ["turn_relay_port_min", "turn_relay_port_max", "turn_port"]
            .iter()
            .any(|name| get(name).is_some())
        {
            let min = number("turn_relay_port_min", self.turn_relay_port_min, 49152);
            let max = number("turn_relay_port_max", self.turn_relay_port_max, 65535);
            let control = number("turn_port", self.turn_port, 3478);
            crate::turn_relay_ports::TurnRelayPortRange { min, max }
                .validate(control)
                .map_err(ChatmailError::config)?;
        }
        let path = |name: &str, field: &Option<String>, default: &str| {
            let raw = get(name)
                .or_else(|| field.clone())
                .unwrap_or_else(|| default.into());
            validate(name, &raw)
        };
        let api = path("admin_path", &self.admin_path, "/api/admin")?;
        let web = path("admin_web_path", &self.admin_web_path, "/admin")?;
        if admin_paths_overlap(&api, &web) {
            return Err(ChatmailError::config(
                "admin_path and admin_web_path must not overlap",
            ));
        }
        Ok(())
    }
}

/// Admin path accepted from a database-backed write (CLI or admin API): the
/// [`validate`] rules plus the admin API's character set and length limit
/// (ASCII letters, digits and `.:/@-_`, at most 253 characters).
pub fn validate_admin_path_write(name: &str, value: &str) -> Result<String> {
    let value = value.trim();
    if value.len() > 253
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".:/@-_".contains(c))
    {
        return Err(ChatmailError::config(format!(
            "invalid value for setting {name}"
        )));
    }
    validate(name, value)
}

/// True when one admin path equals or nests inside the other (both normalized).
pub fn admin_paths_overlap(api: &str, web: &str) -> bool {
    api == web || api.starts_with(&format!("{web}/")) || web.starts_with(&format!("{api}/"))
}

impl crate::AppConfig {
    pub fn apply_deployment_environment(&mut self) -> Result<()> {
        self.apply_deployment_environment_from(|key| std::env::var(key).ok())
    }

    /// [`Self::apply_deployment_environment`] with an injectable lookup (tests).
    pub fn apply_deployment_environment_from(
        &mut self,
        get: impl Fn(&str) -> Option<String>,
    ) -> Result<()> {
        self.settings_policy.apply_environment(&get)?;
        // A listener address is an explicit bind choice: it protects both the
        // port and the bind mode of that service.
        for (env, service, field) in [
            ("CHATMAIL_SMTP_ADDR", "smtp", &mut self.smtp_listen),
            ("CHATMAIL_IMAP_ADDR", "imap", &mut self.imap_listen),
            ("CHATMAIL_HTTP_ADDR", "http", &mut self.http_listen),
        ] {
            if let Some(addr) = get(env) {
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

    /// Listener lines, `alpn_*` and other legacy module directives written by the
    /// installer are file defaults: they must not lock any setting.
    const LEGACY_CONF: &str = r#"
hostname mail.example.org
storage.imapsql local_mailboxes {
    retention 24h
    appendlimit 100M
}
smtp tcp://0.0.0.0:25 {
    max_message_size 100M
}
submission tls://0.0.0.0:465 tcp://0.0.0.0:587 {
}
imap tls://0.0.0.0:993 tcp://0.0.0.0:143 {
    turn_enable yes
    turn_server 203.0.113.1
    turn_port 3478
    turn_secret s3cret
    turn_ttl 86400
    iroh_relay_url http://203.0.113.1:3340
}
turn udp://0.0.0.0:3478 tcp://0.0.0.0:3478 {
    realm 203.0.113.1
    secret s3cret
    relay_ip 203.0.113.1
}
chatmail tcp://0.0.0.0:80 {
    language fa
    ss_addr "0.0.0.0:8388"
    ss_password "pw"
    ss_cipher "aes-128-gcm"
}
chatmail tls://0.0.0.0:443 {
    alpn_smtp smtp
    alpn_imap imap
    language fa
}
"#;

    #[test]
    fn legacy_directives_and_listen_lines_do_not_lock() {
        let cfg = crate::parse_maddy_config(LEGACY_CONF).unwrap();
        assert!(cfg.deployment_policy().unwrap().0.is_empty());
        // Still parsed as the bind and file defaults.
        assert_eq!(cfg.smtp_listen.as_deref(), Some("0.0.0.0:25"));
        assert!(cfg.alpn_imap.is_some() && cfg.alpn_smtp.is_some());
        assert_eq!(cfg.retention.as_deref(), Some("24h"));
        assert_eq!(cfg.turn_port, 3478);
        assert!(cfg.ss_configured());
    }

    #[test]
    fn settings_block_locks_only_named_keys() {
        let conf = format!(
            "{LEGACY_CONF}\nsettings {{\n    smtp_port 2525\n    shared_port_imap false\n}}\n"
        );
        let cfg = crate::parse_maddy_config(&conf).unwrap();
        let policy = cfg.deployment_policy().unwrap();
        let keys: Vec<_> = policy.0.keys().map(String::as_str).collect();
        assert_eq!(keys, ["__SHARED_PORT_IMAP__", "__SMTP_PORT__"]);
        assert!(policy.check_writable(&["__SMTP_LOCAL_ONLY__"]).is_ok());
        assert!(policy.check_writable(&["__SHARED_PORT_SMTP__"]).is_ok());
        assert!(policy.check_writable(&["__SMTP_PORT__"]).is_err());
    }

    #[test]
    fn environment_port_and_listen_address_lock() {
        let mut cfg = crate::parse_maddy_config(LEGACY_CONF).unwrap();
        cfg.apply_deployment_environment_from(|key| {
            (key == "CHATMAIL_IMAP_PORT").then(|| "1143".to_string())
        })
        .unwrap();
        let sources = cfg.settings_policy.sources();
        assert_eq!(sources["__IMAP_PORT__"], "environment:CHATMAIL_IMAP_PORT");
        // A port variable does not imply a bind mode.
        assert!(!sources.contains_key("__IMAP_LOCAL_ONLY__"));

        let mut cfg = crate::parse_maddy_config(LEGACY_CONF).unwrap();
        cfg.apply_deployment_environment_from(|key| {
            (key == "CHATMAIL_SMTP_ADDR").then(|| "127.0.0.1:2525".to_string())
        })
        .unwrap();
        let policy = &cfg.settings_policy.0;
        assert_eq!(policy["__SMTP_PORT__"].value, "2525");
        assert_eq!(policy["__SMTP_LOCAL_ONLY__"].value, "true");
        assert_eq!(
            policy["__SMTP_PORT__"].source,
            "environment:CHATMAIL_SMTP_ADDR"
        );
        assert_eq!(cfg.smtp_listen.as_deref(), Some("127.0.0.1:2525"));
    }

    #[test]
    fn legacy_turn_range_keeps_runtime_handling_but_protected_range_is_checked() {
        let legacy = "turn udp://0.0.0.0:3478 {\n    secret s\n    relay_port_min 51000\n    relay_port_max 50000\n}\n";
        assert!(crate::parse_maddy_config(legacy).is_ok());
        let protected = format!("{legacy}settings {{\n    turn_relay_port_max 50500\n}}\n");
        assert!(crate::parse_maddy_config(&protected).is_err());
    }

    #[test]
    fn legacy_admin_paths_join_the_overlap_check() {
        let conf = "chatmail tcp://0.0.0.0:80 {\n    admin_path /admin/api\n}\n";
        let err = crate::parse_maddy_config(conf).unwrap_err().to_string();
        assert!(err.contains("must not overlap"), "{err}");
        let conf = "chatmail tcp://0.0.0.0:80 {\n    admin_path /private-api\n}\n";
        assert!(crate::parse_maddy_config(conf).is_ok());
    }

    #[test]
    fn admin_path_writes_use_the_api_character_set() {
        assert_eq!(
            validate_admin_path_write("admin_web_path", "/panel/").unwrap(),
            "/panel"
        );
        for bad in ["/panel+1", "/a b", "/x%2e"] {
            assert!(
                validate_admin_path_write("admin_web_path", bad).is_err(),
                "{bad}"
            );
        }
        let long = format!("/{}", "a".repeat(253));
        assert!(validate_admin_path_write("admin_web_path", &long).is_err());
        assert!(validate("admin_web_path", "/panel+1").is_ok());
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
