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

//! Runtime mail port overrides from the settings DB (Madmail `hydrateCache` parity).

use std::collections::HashMap;

use chatmail_config::DbMailPorts;
use chatmail_types::Result;

use crate::{get_bool_setting, get_setting, settings_keys, DbPool};

fn pick(map: &HashMap<String, String>, key: &str) -> Option<String> {
    map.get(key).filter(|s| !s.trim().is_empty()).cloned()
}

fn listener_enabled(map: &HashMap<String, String>, key: &str) -> bool {
    map.get(key)
        .map(|v| !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}

/// Build [`DbMailPorts`] from a settings map (e.g. [`get_settings_many`](crate::get_settings_many)).
pub fn db_ports_from_settings(map: &HashMap<String, String>) -> DbMailPorts {
    DbMailPorts {
        smtp_port: pick(map, settings_keys::SMTP_PORT),
        submission_port: pick(map, settings_keys::SUBMISSION_PORT),
        submission_tls_port: pick(map, settings_keys::SUBMISSION_TLS_PORT),
        imap_port: pick(map, settings_keys::IMAP_PORT),
        imap_tls_port: pick(map, settings_keys::IMAP_TLS_PORT),
        dclogin_imap_security: pick(map, settings_keys::DCLOGIN_IMAP_SECURITY),
        dclogin_smtp_security: pick(map, settings_keys::DCLOGIN_SMTP_SECURITY),
        http_port: pick(map, settings_keys::HTTP_PORT),
        https_port: pick(map, settings_keys::HTTPS_PORT),
        http_enabled: listener_enabled(map, settings_keys::HTTP_ENABLED),
        https_enabled: listener_enabled(map, settings_keys::HTTPS_ENABLED),
    }
}

/// Load port and dclogin security overrides set via the admin API.
pub async fn load_mail_port_overrides(pool: &DbPool) -> Result<DbMailPorts> {
    Ok(DbMailPorts {
        smtp_port: get_setting(pool, settings_keys::SMTP_PORT).await?,
        submission_port: get_setting(pool, settings_keys::SUBMISSION_PORT).await?,
        submission_tls_port: get_setting(pool, settings_keys::SUBMISSION_TLS_PORT).await?,
        imap_port: get_setting(pool, settings_keys::IMAP_PORT).await?,
        imap_tls_port: get_setting(pool, settings_keys::IMAP_TLS_PORT).await?,
        dclogin_imap_security: get_setting(pool, settings_keys::DCLOGIN_IMAP_SECURITY).await?,
        dclogin_smtp_security: get_setting(pool, settings_keys::DCLOGIN_SMTP_SECURITY).await?,
        http_port: get_setting(pool, settings_keys::HTTP_PORT).await?,
        https_port: get_setting(pool, settings_keys::HTTPS_PORT).await?,
        http_enabled: get_bool_setting(pool, settings_keys::HTTP_ENABLED, true).await?,
        https_enabled: get_bool_setting(pool, settings_keys::HTTPS_ENABLED, true).await?,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn db_ports_from_settings_maps_known_keys() {
        let mut map = HashMap::new();
        map.insert(settings_keys::SMTP_PORT.to_string(), "2525".to_string());
        map.insert(
            settings_keys::SUBMISSION_PORT.to_string(),
            "587".to_string(),
        );
        map.insert(
            settings_keys::SUBMISSION_TLS_PORT.to_string(),
            "465".to_string(),
        );
        map.insert(settings_keys::IMAP_PORT.to_string(), "143".to_string());
        map.insert(settings_keys::IMAP_TLS_PORT.to_string(), "993".to_string());
        map.insert(
            settings_keys::DCLOGIN_IMAP_SECURITY.to_string(),
            "starttls".to_string(),
        );
        map.insert(
            settings_keys::DCLOGIN_SMTP_SECURITY.to_string(),
            "ssl".to_string(),
        );
        map.insert(settings_keys::HTTP_PORT.to_string(), "8080".to_string());
        map.insert(settings_keys::HTTPS_PORT.to_string(), "8443".to_string());

        let ports = db_ports_from_settings(&map);
        assert_eq!(ports.smtp_port.as_deref(), Some("2525"));
        assert_eq!(ports.submission_port.as_deref(), Some("587"));
        assert_eq!(ports.submission_tls_port.as_deref(), Some("465"));
        assert_eq!(ports.imap_port.as_deref(), Some("143"));
        assert_eq!(ports.imap_tls_port.as_deref(), Some("993"));
        assert_eq!(ports.dclogin_imap_security.as_deref(), Some("starttls"));
        assert_eq!(ports.dclogin_smtp_security.as_deref(), Some("ssl"));
        assert_eq!(ports.http_port.as_deref(), Some("8080"));
        assert_eq!(ports.https_port.as_deref(), Some("8443"));
    }

    #[test]
    fn db_ports_from_settings_ignores_blank_values() {
        let mut map = HashMap::new();
        map.insert(settings_keys::SMTP_PORT.to_string(), "  ".to_string());
        map.insert(settings_keys::IMAP_PORT.to_string(), String::new());

        let ports = db_ports_from_settings(&map);
        assert!(ports.smtp_port.is_none());
        assert!(ports.imap_port.is_none());
    }

    #[test]
    fn db_ports_from_settings_empty_map_is_default() {
        let ports = db_ports_from_settings(&HashMap::new());
        assert_eq!(ports, DbMailPorts::default());
    }

    #[test]
    fn db_ports_from_settings_http_https_enabled_flags() {
        let mut map = HashMap::new();
        map.insert(
            settings_keys::HTTP_ENABLED.to_string(),
            "false".to_string(),
        );
        map.insert(
            settings_keys::HTTPS_ENABLED.to_string(),
            "true".to_string(),
        );
        let ports = db_ports_from_settings(&map);
        assert!(!ports.http_enabled);
        assert!(ports.https_enabled);
    }
}
