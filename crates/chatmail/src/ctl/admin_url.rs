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

use chatmail_config::AppConfig;
use chatmail_db::settings_keys::{ADMIN_PATH, HTTPS_PORT, SMTP_HOSTNAME};

/// Build admin API URL (Madmail `buildAdminURL` in `ctl/admin_token.go`).
pub fn build_admin_url(config: &AppConfig, settings: &HashMap<String, String>) -> String {
    let mut host = config
        .hostname
        .clone()
        .or_else(|| config.primary_domain.clone())
        .unwrap_or_else(|| "your-server".into());
    if let Some(v) = settings.get(SMTP_HOSTNAME) {
        if !v.is_empty() {
            host = v.clone();
        }
    }
    let host = host.trim_matches(|c| c == '[' || c == ']').to_string();

    let https_port = settings
        .get(HTTPS_PORT)
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| "443".into());

    let http_port = listen_port(config.http_listen.as_deref()).unwrap_or_else(|| "80".into());
    let https_port = listen_port(config.http_tls_listen.as_deref()).unwrap_or(https_port);

    let path = effective_admin_path(config, settings);

    // Local dev: HTTP listener on a non-443 port (e.g. 8080) — show http URL for admin-token / UI.
    if https_port == "443" && http_port != "80" && http_port != "443" {
        return format!("http://{host}:{http_port}{path}");
    }

    if https_port == "443" {
        format!("https://{host}{path}")
    } else {
        format!("https://{host}:{https_port}{path}")
    }
}

/// Public site origin (HTTPS or local HTTP) for the admin-web dashboard.
///
/// The admin API path is removed even when it is not the built-in `/api/admin`,
/// so a dashboard link stays `https://host/admin/` after the API moves.
pub fn build_site_base_url(config: &AppConfig, settings: &HashMap<String, String>) -> String {
    let admin = build_admin_url(config, settings);
    let path = effective_admin_path(config, settings);
    let path = path.trim_end_matches('/');
    let admin = admin.trim_end_matches('/');
    admin
        .strip_suffix(path)
        .unwrap_or(admin)
        .trim_end_matches('/')
        .to_string()
}

fn effective_admin_path(config: &AppConfig, settings: &HashMap<String, String>) -> String {
    let admin_path = settings
        .get(ADMIN_PATH)
        .filter(|s| !s.is_empty())
        .cloned()
        .or_else(|| config.admin_path.clone())
        .unwrap_or_else(|| "/api/admin".into());
    if admin_path.starts_with('/') {
        admin_path
    } else {
        format!("/{admin_path}")
    }
}

fn listen_port(addr: Option<&str>) -> Option<String> {
    let addr = addr?.trim();
    let (_, port) = addr.rsplit_once(':')?;
    if port.chars().all(|c| c.is_ascii_digit()) {
        Some(port.to_string())
    } else {
        None
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn default_https_443() {
        let cfg = AppConfig::default();
        let url = build_admin_url(&cfg, &HashMap::new());
        assert_eq!(url, "https://your-server/api/admin");
    }

    #[test]
    fn local_http_listen_8080() {
        let mut cfg = AppConfig::default();
        cfg.hostname = Some("127.0.0.1".into());
        cfg.http_listen = Some("0.0.0.0:8080".into());
        let url = build_admin_url(&cfg, &HashMap::new());
        assert_eq!(url, "http://127.0.0.1:8080/api/admin");
    }

    #[test]
    fn site_base_strips_a_custom_admin_api_path() {
        let mut cfg = AppConfig::default();
        cfg.hostname = Some("mail.example".into());
        let mut settings = HashMap::new();
        settings.insert(ADMIN_PATH.to_string(), "/api/offline".into());
        assert_eq!(
            build_admin_url(&cfg, &settings),
            "https://mail.example/api/offline"
        );
        assert_eq!(build_site_base_url(&cfg, &settings), "https://mail.example");
        let site = build_site_base_url(&cfg, &settings);
        let web = format!("{}/admin/", site.trim_end_matches('/'));
        assert_eq!(web, "https://mail.example/admin/");
    }

    #[test]
    fn site_base_keeps_the_default_origin() {
        let cfg = AppConfig::default();
        assert_eq!(
            build_site_base_url(&cfg, &HashMap::new()),
            "https://your-server"
        );
    }
}
