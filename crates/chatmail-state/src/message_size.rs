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

use std::sync::atomic::{AtomicU64, Ordering};

use chatmail_config::{
    effective_max_message_bytes, parse_data_size, resolve_max_message_bytes, AppConfig,
};
use chatmail_db::{get_setting, set_setting, settings_keys, DbPool};
use chatmail_types::Result;

/// Runtime SMTP/IMAP/WebSMTP message size cap (config + DB overrides).
#[derive(Debug)]
pub struct MessageSizeLimit {
    config_bytes: u64,
    effective_bytes: AtomicU64,
}

impl MessageSizeLimit {
    pub fn new(config: &AppConfig) -> Self {
        let config_bytes = effective_max_message_bytes(config);
        Self {
            config_bytes,
            effective_bytes: AtomicU64::new(config_bytes),
        }
    }

    pub fn effective(&self) -> u64 {
        self.effective_bytes.load(Ordering::Relaxed)
    }

    pub fn config_bytes(&self) -> u64 {
        self.config_bytes
    }

    pub async fn hydrate(&self, pool: &DbPool, config: &AppConfig) -> Result<()> {
        self.refresh_from_db(pool, config).await
    }

    pub async fn refresh_from_db(&self, pool: &DbPool, config: &AppConfig) -> Result<()> {
        let config_eff = effective_max_message_bytes(config);
        let append = get_setting(pool, settings_keys::APPENDLIMIT).await?;
        let max = get_setting(pool, settings_keys::MAX_MESSAGE_SIZE).await?;
        let eff = resolve_max_message_bytes(config_eff, append.as_deref(), max.as_deref())?;
        self.effective_bytes.store(eff, Ordering::Relaxed);
        Ok(())
    }

    /// Set both DB keys to the same value and refresh the in-memory cap.
    pub async fn set_limit(
        &self,
        pool: &DbPool,
        config: &AppConfig,
        size: &str,
    ) -> Result<u64> {
        parse_data_size(size)?;
        set_setting(pool, settings_keys::APPENDLIMIT, size).await?;
        set_setting(pool, settings_keys::MAX_MESSAGE_SIZE, size).await?;
        self.refresh_from_db(pool, config).await?;
        Ok(self.effective())
    }

    /// Clear DB overrides and revert to config file limits.
    pub async fn reset_limit(&self, pool: &DbPool, config: &AppConfig) -> Result<u64> {
        chatmail_db::delete_setting(pool, settings_keys::APPENDLIMIT).await?;
        chatmail_db::delete_setting(pool, settings_keys::MAX_MESSAGE_SIZE).await?;
        self.refresh_from_db(pool, config).await?;
        Ok(self.effective())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_config::DEFAULT_MAX_MESSAGE_BYTES;
    use chatmail_db::init_memory_db;

    #[tokio::test]
    async fn message_size_hydrate_from_db_seed() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::seed_install_defaults(&pool).await.unwrap();
        let cfg = AppConfig::default();
        let lim = MessageSizeLimit::new(&cfg);
        lim.hydrate(&pool, &cfg).await.unwrap();
        assert_eq!(lim.effective(), 100 * 1024 * 1024);
    }

    #[tokio::test]
    async fn message_size_set_and_reset() {
        let pool = init_memory_db().await.unwrap();
        let cfg = AppConfig::default();
        let lim = MessageSizeLimit::new(&cfg);
        lim.set_limit(&pool, &cfg, "50M").await.unwrap();
        assert_eq!(lim.effective(), 50 * 1024 * 1024);
        lim.reset_limit(&pool, &cfg).await.unwrap();
        assert_eq!(lim.effective(), DEFAULT_MAX_MESSAGE_BYTES);
    }

    #[tokio::test]
    async fn message_size_db_mismatch_uses_min() {
        let pool = init_memory_db().await.unwrap();
        set_setting(&pool, settings_keys::APPENDLIMIT, "80M")
            .await
            .unwrap();
        set_setting(&pool, settings_keys::MAX_MESSAGE_SIZE, "40M")
            .await
            .unwrap();
        let cfg = AppConfig::default();
        let lim = MessageSizeLimit::new(&cfg);
        lim.refresh_from_db(&pool, &cfg).await.unwrap();
        assert_eq!(lim.effective(), 40 * 1024 * 1024);
    }
}
