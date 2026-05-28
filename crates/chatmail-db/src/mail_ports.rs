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

use crate::{get_setting, settings_keys, DbPool};

fn pick(map: &HashMap<String, String>, key: &str) -> Option<String> {
    map.get(key)
        .filter(|s| !s.trim().is_empty())
        .cloned()
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
    })
}
