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

//! Runtime flag: allow non-local RCPT on unauthenticated inbound SMTP.

use std::sync::atomic::{AtomicBool, Ordering};

use chatmail_config::{parse_bool_str, AppConfig};
use chatmail_db::{get_setting, settings_keys, DbPool};
use chatmail_types::Result;

/// Effective `allow_inbound_remote_rcpt` (file default + optional admin DB override).
#[derive(Debug)]
pub struct InboundRemoteRcptFlag {
    effective: AtomicBool,
}

impl InboundRemoteRcptFlag {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            effective: AtomicBool::new(config.allow_inbound_remote_rcpt),
        }
    }

    /// Whether inbound (unauthenticated) sessions may accept non-local RCPT.
    pub fn allowed(&self) -> bool {
        self.effective.load(Ordering::Relaxed)
    }

    /// Update the live flag (admin toggle / tests).
    pub fn set(&self, allowed: bool) {
        self.effective.store(allowed, Ordering::Relaxed);
    }

    /// DB override when set; otherwise file config.
    pub async fn hydrate(&self, pool: &DbPool, config: &AppConfig) -> Result<()> {
        let allowed = match get_setting(pool, settings_keys::ALLOW_INBOUND_REMOTE_RCPT).await? {
            Some(v) => parse_bool_str(&v),
            None => config.allow_inbound_remote_rcpt,
        };
        self.set(allowed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::{init_memory_db, set_setting};

    #[tokio::test]
    async fn hydrate_uses_file_default_when_db_unset() {
        let pool = init_memory_db().await.unwrap();
        let cfg = AppConfig {
            allow_inbound_remote_rcpt: true,
            ..Default::default()
        };
        let flag = InboundRemoteRcptFlag::new(&cfg);
        flag.hydrate(&pool, &cfg).await.unwrap();
        assert!(flag.allowed());
    }

    #[tokio::test]
    async fn hydrate_db_overrides_file() {
        let pool = init_memory_db().await.unwrap();
        let cfg = AppConfig {
            allow_inbound_remote_rcpt: true,
            ..Default::default()
        };
        set_setting(&pool, settings_keys::ALLOW_INBOUND_REMOTE_RCPT, "false")
            .await
            .unwrap();
        let flag = InboundRemoteRcptFlag::new(&cfg);
        flag.hydrate(&pool, &cfg).await.unwrap();
        assert!(!flag.allowed());
    }

    #[tokio::test]
    async fn hydrate_db_true_overrides_file_false() {
        let pool = init_memory_db().await.unwrap();
        let cfg = AppConfig {
            allow_inbound_remote_rcpt: false,
            ..Default::default()
        };
        set_setting(&pool, settings_keys::ALLOW_INBOUND_REMOTE_RCPT, "true")
            .await
            .unwrap();
        let flag = InboundRemoteRcptFlag::new(&cfg);
        assert!(!flag.allowed(), "starts from file default");
        flag.hydrate(&pool, &cfg).await.unwrap();
        assert!(flag.allowed(), "DB true must win over file false");
    }

    #[tokio::test]
    async fn set_toggles_live_without_hydrate() {
        let cfg = AppConfig::default();
        let flag = InboundRemoteRcptFlag::new(&cfg);
        assert!(!flag.allowed());
        flag.set(true);
        assert!(flag.allowed());
        flag.set(false);
        assert!(!flag.allowed());
    }
}
