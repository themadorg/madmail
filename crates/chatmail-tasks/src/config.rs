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

//! Maintenance intervals from `maddy.conf` (`storage.imapsql`).

use std::time::Duration;

use chatmail_config::{parse_duration, AppConfig};
use chatmail_db::{effective_message_retention, DbPool};
use chatmail_types::{ChatmailError, Result};

/// How often periodic jobs run in the server process (Madmail: 1 hour).
pub const PERIODIC_INTERVAL: Duration = Duration::from_secs(3600);

/// Auto-purge seen messages when `__AUTO_PURGE_SEEN__` is enabled (Madmail: 15 seconds).
pub const AUTO_PURGE_SEEN_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceConfig {
    pub message_retention: Option<Duration>,
    pub unused_account_retention: Option<Duration>,
}

impl MaintenanceConfig {
    pub fn from_app_config(config: &AppConfig) -> Result<Self> {
        Ok(Self {
            message_retention: optional_duration(config.retention.as_deref())?,
            unused_account_retention: optional_duration(
                config.unused_account_retention.as_deref(),
            )?,
        })
    }

    /// Message file rotation from DB; unused accounts still from `maddy.conf`.
    pub async fn from_runtime(pool: &DbPool, config: &AppConfig) -> Result<Self> {
        let message_retention = effective_message_retention(pool).await?;
        Ok(Self {
            message_retention,
            unused_account_retention: optional_duration(
                config.unused_account_retention.as_deref(),
            )?,
        })
    }

    pub fn periodic_jobs_enabled(&self) -> bool {
        self.message_retention.is_some() || self.unused_account_retention.is_some()
    }
}

fn optional_duration(raw: Option<&str>) -> Result<Option<Duration>> {
    let Some(s) = raw else {
        return Ok(None);
    };
    let s = s.trim();
    if s.is_empty() || s == "0" {
        return Ok(None);
    }
    parse_duration(s).map(Some).map_err(|_| {
        ChatmailError::config(format!("invalid duration in config: {s:?} (use e.g. 24h, 7d)"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_retention_disables_job() {
        let mut cfg = AppConfig::default();
        cfg.retention = Some("0".into());
        let m = MaintenanceConfig::from_app_config(&cfg).unwrap();
        assert!(m.message_retention.is_none());
    }
}
