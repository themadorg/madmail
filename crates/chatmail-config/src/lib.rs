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

pub mod autoconfig;
pub mod cli;
pub mod client_mail;
pub mod config_autocert;
pub mod config_www;
pub mod credential_policy;
pub mod data_size;
pub mod db_path;
pub mod install_cli;
pub mod maddy;
mod madmail_lexer;
mod madmail_parse;
pub mod parse;
pub mod paths;
pub mod queue;

pub use config_autocert::update_config_autocert;
pub use config_www::update_config_www_dir;

use std::path::PathBuf;

pub use autoconfig::{build_autoconfig_xml, AutoconfigParams};
pub use cli::{
    AdminWebCommand, Args, Cli, Command, CompletionShell, EndpointCacheCommand, FederationCommand,
    LanguageCommand, PortCommand, PortServiceCommand, PushCommand, RegistrationCommand,
    RegistrationTokensCommand, ServiceToggleCommand, SharingCommand, TasksCommand, UninstallArgs,
};
pub use client_mail::{
    build_dclogin_link, client_connect_host, effective_http_listen, effective_http_plain_listen,
    effective_http_tls_listen, effective_imap_listen, effective_imap_plain_listen,
    effective_imap_tls_listen, effective_smtp_listen, effective_submission_plain_listen,
    effective_submission_tls_listen, effective_tls_pem_paths, listeners_need_tls_cert,
    port_from_listen, DbMailPorts, DcloginMailSettings, RuntimeListeners,
};
pub use credential_policy::CredentialPolicy;
pub use data_size::{
    effective_default_quota_bytes, effective_max_message_bytes, format_data_size, parse_data_size,
    resolve_max_message_bytes, DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_MAX_MESSAGE_SIZE,
    DEFAULT_QUOTA_BYTES,
};
pub use db_path::{
    effective_app_db_path, effective_database_config, DatabaseConfig, DbDriver, CHATMAIL_RS_DB,
    MADMAIL_CREDENTIALS_DB,
};
pub use maddy::{
    maddy_listen_to_socket_addr, parse_duration, parse_maddy_conf_str, parse_maddy_config,
    resolve_state_path, ParseDurationError,
};
pub use madmail_parse::{read as read_maddy_ast, ConfigAst, Node, ParseError};
pub use parse::load_config;
pub use paths::{
    apply_cli_defaults, detect_default_config_path, detect_default_state_dir,
    is_local_dev_state_dir,
};
pub use queue::QueueSettings;

/// Server configuration (static `maddy.conf` / `chatmail.toml` + derived paths).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppConfig {
    pub hostname: Option<String>,
    pub primary_domain: Option<String>,
    pub local_domains: Option<String>,
    pub public_ip: Option<String>,
    pub state_dir: Option<PathBuf>,
    pub runtime_dir: Option<PathBuf>,
    pub tls_mode: Option<String>,
    /// ACME contact email (`acme_email` directive / `chatmail.toml`).
    pub acme_email: Option<String>,
    /// `tls file <cert> <key>` from `maddy.conf`.
    pub tls_cert_path: Option<PathBuf>,
    pub tls_key_path: Option<PathBuf>,
    pub debug: bool,
    pub log_target: Option<String>,

    /// `auth.pass_table` (JIT / auto_create).
    pub auth_auto_create: bool,
    pub jit_domain: Option<String>,
    /// `auth.pass_table` → `table sql_table` `driver` (`sqlite3`, `postgres`, …).
    pub credentials_driver: Option<String>,
    pub credentials_dsn: Option<String>,

    /// `storage.imapsql`.
    pub imapsql_driver: Option<String>,
    pub imapsql_dsn: Option<String>,
    pub default_quota: Option<String>,
    pub retention: Option<String>,
    /// `storage.imapsql unused_account_retention` — delete never-logged-in accounts (Madmail).
    pub unused_account_retention: Option<String>,
    pub appendlimit: Option<String>,
    /// `smtp` / `submission` `max_message_size` (e.g. `100M`).
    pub max_message_size: Option<String>,
    /// `storage.imapsql mail_fsync` — `always`, `optimized`, or `never` (Dovecot parity).
    pub mail_fsync: Option<String>,
    /// `storage.imapsql blob_dedup` — content-addressed dedup for identical payloads.
    pub blob_dedup: Option<String>,

    /// `chatmail` HTTP endpoint.
    pub mail_domain: Option<String>,
    pub mx_domain: Option<String>,
    /// `username_length` — auto-generated localpart length (Madmail default: 8).
    pub username_length: Option<u32>,
    /// `password_length` — auto-generated password length (Madmail default: 16).
    pub password_length: Option<u32>,
    /// `min_username_length` — minimum localpart length (Madmail example: 3; madmail-v2 default: 8).
    pub min_username_length: Option<u32>,
    /// `max_username_length` — maximum localpart length (Madmail default: 20).
    pub max_username_length: Option<u32>,
    /// `password_min_length` — minimum password length on JIT account creation (default: 8).
    pub password_min_length: Option<u32>,
    /// Default www UI language (`en`, `fa`, `ru`, `es`) when not set in DB.
    pub language: Option<String>,
    /// External www directory (`chatmail { www_dir ... }` / `html-serve`).
    /// Unset = default site from embedded RAM in the binary (fast; no disk reads).
    pub www_dir: Option<PathBuf>,
    /// `admin_path` (default `/api/admin`).
    pub admin_path: Option<String>,
    /// `admin_web_path` — URL path for the embedded admin-web SPA (e.g. `/admin`).
    pub admin_web_path: Option<String>,
    /// `admin_token` — literal token or `disabled` to turn off the admin API.
    pub admin_token: Option<String>,

    /// Listen addresses (`host:port`), parsed from `tcp://` / `tls://` lines.
    pub smtp_listen: Option<String>,
    pub submission_listen: Option<String>,
    pub submission_tls_listen: Option<String>,
    pub imap_listen: Option<String>,
    pub imap_tls_listen: Option<String>,
    pub http_listen: Option<String>,
    pub http_tls_listen: Option<String>,

    /// `openmetrics tcp://…` — Prometheus scrape bind (`/metrics`).
    pub openmetrics_listen: Option<String>,

    /// `target.queue remote_queue` — outbound retry queue (Madmail defaults).
    pub queue: QueueSettings,

    /// IMAP `turn_*` directives (TURN discovery for Delta Chat calls).
    pub turn_enable: bool,
    pub turn_server: Option<String>,
    pub turn_port: u16,
    pub turn_secret: Option<String>,
    pub turn_ttl: u64,

    /// `turn udp://… tcp://… { }` endpoint — relay listener addresses.
    pub turn_listen_udp: Option<String>,
    pub turn_listen_tcp: Option<String>,
    pub turn_realm: Option<String>,
    pub turn_relay_ip: Option<String>,
    /// Verbose turn-rs logs (`turn { debug }` or `CHATMAIL_TURN_DEBUG=1`).
    pub turn_debug: bool,
    /// Fake STUN mapped IP in Binding responses; real relay (see `turn-test.md`).
    pub turn_test_force_relay: bool,

    /// IMAP `iroh_relay_url` — advertised via METADATA when set (WebXDC realtime).
    pub iroh_relay_url: Option<String>,
    /// Explicit enable (install sets `iroh_relay_url`; toggle via `__IROH_ENABLED__`).
    pub iroh_enable: bool,
    /// Default relay HTTP port when URL is derived from `public_ip` / hostname.
    pub iroh_port: u16,

    /// `chatmail { ss_addr … ss_password … ss_cipher … }` — Shadowsocks proxy.
    pub ss_addr: Option<String>,
    pub ss_password: Option<String>,
    /// Default `aes-128-gcm` when SS is enabled.
    pub ss_cipher: Option<String>,
    pub ss_cert_path: Option<PathBuf>,
    pub ss_key_path: Option<PathBuf>,
    /// `ss_allowed_ports` list; empty = Madmail defaults + discovered mail ports.
    pub ss_allowed_ports: Vec<String>,
}

impl AppConfig {
    /// Shadowsocks is configured in `maddy.conf` (`ss_addr` + `ss_password`).
    pub fn ss_configured(&self) -> bool {
        self.ss_addr.as_ref().is_some_and(|s| !s.is_empty())
            && self.ss_password.as_ref().is_some_and(|s| !s.is_empty())
    }

    /// Canonical `$(primary_domain)` (IPs as `[x.x.x.x]`).
    pub fn effective_primary_domain(&self, hostname_fallback: &str) -> String {
        let raw = self.primary_domain.as_deref().unwrap_or(hostname_fallback);
        chatmail_types::wrap_ip_domain(raw)
    }

    /// All domains this server accepts locally (`$(local_domains)` + bracket/bare IP aliases).
    pub fn effective_local_domains(&self, hostname_fallback: &str) -> Vec<String> {
        let primary = self.effective_primary_domain(hostname_fallback);
        chatmail_types::build_local_domains(&primary, self.local_domains.as_deref())
    }

    /// Domain for new accounts / dclogin (`user@domain` or `user@[1.2.3.4]`).
    ///
    /// Install mode (`primary_domain` / `mail_domain`) wins over the HTTP `Host`
    /// so an IP server stays `@[127.0.0.1]` even when the page is opened as
    /// `http://localhost:8080`. Without explicit config, uses `Host`, then
    /// `hostname`, then `127.0.0.1`.
    pub fn effective_registration_domain(&self, http_host: Option<&str>) -> String {
        if let Some(p) = self.primary_domain.as_deref().filter(|s| !s.is_empty()) {
            return chatmail_types::wrap_ip_domain(p);
        }
        if let Some(md) = self.mail_domain.as_deref().filter(|s| !s.is_empty()) {
            return chatmail_types::wrap_ip_domain(md);
        }
        if let Some(host) = http_host {
            let host = host.split(':').next().unwrap_or(host).trim();
            if !host.is_empty() {
                return chatmail_types::wrap_ip_domain(host);
            }
        }
        let fallback = self.hostname.as_deref().unwrap_or("127.0.0.1");
        chatmail_types::wrap_ip_domain(fallback)
    }

    /// JIT / login domain restriction (`auth.pass_table` `jit_domain`).
    pub fn effective_jit_domain(&self, hostname_fallback: &str) -> Option<String> {
        let raw = self
            .jit_domain
            .as_deref()
            .or(self.primary_domain.as_deref())
            .unwrap_or(hostname_fallback);
        Some(chatmail_types::wrap_ip_domain(raw))
    }

    /// Effective SMTP submission/plain listen for madmail-v2 dev server.
    pub fn smtp_addr(&self) -> Option<&str> {
        self.submission_listen
            .as_deref()
            .or(self.smtp_listen.as_deref())
    }

    pub fn imap_addr(&self) -> Option<&str> {
        self.imap_listen.as_deref()
    }

    pub fn http_addr(&self) -> Option<&str> {
        self.http_listen.as_deref()
    }

    /// Hostname or IP advertised in IMAP TURN metadata (`turn_server` or `public_ip`).
    pub fn effective_turn_server(&self, hostname_fallback: &str) -> String {
        self.turn_server
            .clone()
            .or_else(|| self.public_ip.clone())
            .unwrap_or_else(|| hostname_fallback.to_string())
    }

    /// Whether TURN discovery + relay are configured in static config.
    pub fn turn_configured(&self) -> bool {
        self.turn_enable && self.turn_secret.as_ref().is_some_and(|s| !s.is_empty())
    }

    /// Whether Iroh relay + IMAP discovery are configured in static config.
    pub fn iroh_configured(&self) -> bool {
        self.iroh_enable || self.iroh_relay_url.as_ref().is_some_and(|s| !s.is_empty())
    }

    /// ACME contact email: configured `acme_email`, else `admin@<domain>`.
    pub fn effective_acme_email(&self, domain: &str) -> String {
        if let Some(email) = self.acme_email.as_deref().filter(|s| !s.is_empty()) {
            return email.to_string();
        }
        let bare = domain.trim_matches(|c| c == '[' || c == ']');
        format!("admin@{bare}")
    }

    /// Default Iroh relay URL from `iroh_relay_url`, else `http://{host}:{port}`.
    pub fn effective_iroh_relay_url(&self, hostname_fallback: &str) -> Option<String> {
        if let Some(url) = self.iroh_relay_url.as_ref().filter(|s| !s.is_empty()) {
            return Some(url.clone());
        }
        if !self.iroh_enable {
            return None;
        }
        let host = self
            .public_ip
            .clone()
            .or_else(|| self.hostname.clone())
            .unwrap_or_else(|| hostname_fallback.to_string());
        let port = if self.iroh_port == 0 {
            3340
        } else {
            self.iroh_port
        };
        Some(format!("http://{host}:{port}"))
    }
}

/// Resolve state directory: config file `state_dir` overrides CLI default when set.
pub fn resolve_state_dir(cli: PathBuf, config: &AppConfig) -> PathBuf {
    config.state_dir.clone().unwrap_or(cli)
}

pub fn default_state_dir() -> PathBuf {
    detect_default_state_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p1_resolve_state_dir_prefers_config() {
        let cli = PathBuf::from("/var/lib/chatmail");
        let cfg = AppConfig {
            state_dir: Some(PathBuf::from("/from/config")),
            ..Default::default()
        };
        assert_eq!(resolve_state_dir(cli, &cfg), PathBuf::from("/from/config"));
    }

    #[test]
    fn registration_domain_primary_wins_over_http_host() {
        let cfg = AppConfig {
            primary_domain: Some("127.0.0.1".into()),
            mail_domain: Some("localhost".into()),
            ..Default::default()
        };
        assert_eq!(
            cfg.effective_registration_domain(Some("localhost:8080")),
            "[127.0.0.1]"
        );
    }

    #[test]
    fn registration_domain_wraps_ip_from_http_host_without_primary() {
        let cfg = AppConfig::default();
        assert_eq!(
            cfg.effective_registration_domain(Some("127.0.0.1:8080")),
            "[127.0.0.1]"
        );
    }

    #[test]
    fn registration_domain_uses_dns_primary() {
        let cfg = AppConfig {
            primary_domain: Some("a.com".into()),
            ..Default::default()
        };
        assert_eq!(cfg.effective_registration_domain(None), "a.com");
    }

    #[test]
    fn registration_domain_localhost_host_header() {
        let cfg = AppConfig::default();
        assert_eq!(
            cfg.effective_registration_domain(Some("localhost:8080")),
            "localhost"
        );
    }

    #[test]
    fn p1_resolve_state_dir_falls_back_to_cli() {
        let cli = PathBuf::from("/tmp/cli-state");
        let cfg = AppConfig::default();
        assert_eq!(resolve_state_dir(cli.clone(), &cfg), cli);
    }
}
