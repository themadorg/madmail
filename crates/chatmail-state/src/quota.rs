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

use chatmail_db::passwords;
use chatmail_storage::MailboxStore;
use chatmail_types::{ChatmailError, Result};
use dashmap::DashMap;
use chatmail_db::{db_fetch_all, db_fetch_optional, schema::quota_table, DbPool};

use chatmail_db::settings_keys::GLOBAL_QUOTA_USERNAME;

#[derive(Debug)]
pub struct QuotaEntry {
    pub used_bytes: AtomicU64,
    pub max_bytes: u64,
    pub is_default: bool,
}

impl QuotaEntry {
    fn new(used: u64, max: u64, is_default: bool) -> Self {
        Self {
            used_bytes: AtomicU64::new(used),
            max_bytes: max,
            is_default,
        }
    }
}

#[derive(Debug)]
pub struct QuotaCache {
    entries: DashMap<String, QuotaEntry>,
    /// Config fallback when `__GLOBAL_DEFAULT__` is missing or `max_storage <= 0`.
    config_default_max_bytes: u64,
    default_max_bytes: AtomicU64,
}

impl QuotaCache {
    pub fn new(config_default_max_bytes: u64) -> Self {
        Self {
            entries: DashMap::new(),
            config_default_max_bytes,
            default_max_bytes: AtomicU64::new(config_default_max_bytes),
        }
    }

    /// Effective global default (DB override if positive, else config).
    pub fn default_max_bytes(&self) -> u64 {
        self.default_max_bytes.load(Ordering::Relaxed)
    }

    pub fn set_default_max(&self, bytes: u64) {
        self.default_max_bytes.store(bytes, Ordering::Relaxed);
    }

    /// Per-user cap lookup (Madmail `GetQuota` / admin accounts).
    pub fn get_quota(&self, user: &str) -> (u64, u64, bool) {
        let default_max = self.default_max_bytes();
        match self.entries.get(user) {
            Some(entry) => {
                let used = entry.used_bytes.load(Ordering::Relaxed);
                let mut max = entry.max_bytes;
                let is_default = entry.is_default;
                // Cached default with 0 max → use real global default (Madmail cache fix).
                if is_default && max == 0 {
                    max = default_max;
                }
                (used, max, is_default)
            }
            None => (0, default_max, true),
        }
    }

    /// Update per-user or global default cap after admin API writes.
    pub fn set_max_bytes(&self, user: &str, max: u64) {
        if user == GLOBAL_QUOTA_USERNAME {
            let effective = if max > 0 {
                max
            } else {
                self.config_default_max_bytes
            };
            self.set_default_max(effective);
            for mut entry in self.entries.iter_mut() {
                if entry.is_default {
                    entry.max_bytes = effective;
                }
            }
            return;
        }
        if max == 0 {
            self.reset_max(user);
            return;
        }
        let used = self.used_bytes(user);
        self.entries
            .insert(user.to_string(), QuotaEntry::new(used, max, false));
    }

    /// Drop cached quota for a removed account (Madmail `QuotaCache.Invalidate`).
    pub fn invalidate(&self, user: &str) {
        self.entries.remove(user);
    }

    /// Remove per-user override; fall back to global default (Madmail `ResetQuota`).
    pub fn reset_max(&self, user: &str) {
        let default_max = self.default_max_bytes();
        if let Some(mut entry) = self.entries.get_mut(user) {
            entry.max_bytes = default_max;
            entry.is_default = true;
        }
    }

    /// Load quotas from DB and scan maildir usage (Madmail `populateQuotaCache`).
    pub async fn hydrate(&self, pool: &DbPool, store: &MailboxStore) -> Result<()> {
        let default_max = Self::load_default_max_from_db(pool, self.config_default_max_bytes).await?;
        self.set_default_max(default_max);

        let qt = quota_table(pool).await?;
        let sql = format!("SELECT username, max_storage FROM {qt} WHERE username != ?");
        let rows: Vec<(String, i64)> = db_fetch_all!(pool, (String, i64), &sql, GLOBAL_QUOTA_USERNAME)?;

        let quota_map: std::collections::HashMap<String, i64> =
            rows.into_iter().map(|(u, m)| (u, m)).collect();

        let all_users = passwords::list_users(pool).await?;
        for user in &all_users {
            let used = store.maildir_used_bytes(user).await.unwrap_or(0);
            let (max, is_default) = per_user_max(&quota_map, user, default_max);
            self.entries
                .insert(user.clone(), QuotaEntry::new(used, max, is_default));
        }

        for user in quota_map.keys() {
            if self.entries.contains_key(user) {
                continue;
            }
            let used = store.maildir_used_bytes(user).await.unwrap_or(0);
            let (max, is_default) = per_user_max(&quota_map, user, default_max);
            self.entries
                .insert(user.clone(), QuotaEntry::new(used, max, is_default));
        }

        // Maildirs without a credentials row (legacy / manual dirs).
        let mail_root = store.state_dir().join("mail");
        if mail_root.is_dir() {
            let mut read_dir = tokio::fs::read_dir(&mail_root).await?;
            while let Some(entry) = read_dir.next_entry().await? {
                if !entry.file_type().await?.is_dir() {
                    continue;
                }
                let user = entry.file_name().to_string_lossy().into_owned();
                if self.entries.contains_key(&user) {
                    continue;
                }
                let used = store.maildir_used_bytes(&user).await.unwrap_or(0);
                let (max, is_default) = per_user_max(&quota_map, &user, default_max);
                self.entries
                    .insert(user, QuotaEntry::new(used, max, is_default));
            }
        }

        Ok(())
    }

    async fn load_default_max_from_db(pool: &DbPool, config_default: u64) -> Result<u64> {
        let qt = quota_table(pool).await?;
        let sql = format!("SELECT max_storage FROM {qt} WHERE username = ?");
        let row: Option<(i64,)> =
            db_fetch_optional!(pool, (i64,), &sql, GLOBAL_QUOTA_USERNAME)?;
        Ok(match row {
            Some((m,)) if m > 0 => m as u64,
            _ => config_default,
        })
    }

    pub fn record_write(&self, user: &str, bytes: u64) {
        if let Some(entry) = self.entries.get(user) {
            entry
                .used_bytes
                .fetch_add(bytes, Ordering::Relaxed);
            return;
        }
        let (_, max, is_default) = self.get_quota(user);
        self.entries.insert(
            user.to_string(),
            QuotaEntry::new(bytes, max, is_default),
        );
    }

    pub fn used_bytes(&self, user: &str) -> u64 {
        self.get_quota(user).0
    }

    pub fn max_bytes(&self, user: &str) -> u64 {
        self.get_quota(user).1
    }

    pub fn check_quota(&self, user: &str, incoming_bytes: u64) -> Result<()> {
        let (used, max, _) = self.get_quota(user);
        if used.saturating_add(incoming_bytes) > max {
            return Err(ChatmailError::QuotaExceeded {
                user: user.to_string(),
                used,
                incoming: incoming_bytes,
                max,
            });
        }
        // Ensure cache entry exists for subsequent record_write.
        if !self.entries.contains_key(user) {
            self.entries.insert(
                user.to_string(),
                QuotaEntry::new(used, max, true),
            );
        }
        Ok(())
    }
}

fn per_user_max(
    quota_map: &std::collections::HashMap<String, i64>,
    user: &str,
    default_max: u64,
) -> (u64, bool) {
    match quota_map.get(user) {
        Some(&m) if m > 0 => (m as u64, false),
        _ => (default_max, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_config::DEFAULT_QUOTA_BYTES;
    use chatmail_db::{db_execute, init_memory_db};
    use chatmail_storage::MailboxStore;

    /// P2-UT03: 10MB cap rejects 11MB write.
    #[tokio::test]
    async fn p2_ut03_test_quota_exceeded() {
        let cache = QuotaCache::new(10 * 1024 * 1024);
        cache.entries.insert(
            "user@example.org".into(),
            QuotaEntry::new(0, 10 * 1024 * 1024, false),
        );

        let err = cache
            .check_quota("user@example.org", 11 * 1024 * 1024)
            .unwrap_err();
        assert!(matches!(err, ChatmailError::QuotaExceeded { .. }));
    }

    #[tokio::test]
    async fn test_quota_hydrate_from_db() {
        let pool = init_memory_db().await.unwrap();
        db_execute!(
            pool,
            "INSERT INTO quotas (username, max_storage, created_at, first_login_at, last_login_at)
             VALUES ('alice@x.org', 5000, 0, 0, 0)"
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(dir.path());
        let cache = QuotaCache::new(1024);
        cache.hydrate(&pool, &store).await.unwrap();
        cache.check_quota("alice@x.org", 100).unwrap();
        assert_eq!(cache.max_bytes("alice@x.org"), 5000);
    }

    #[tokio::test]
    async fn test_global_default_from_config_when_db_row_missing() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(dir.path());
        let cache = QuotaCache::new(DEFAULT_QUOTA_BYTES);
        cache.hydrate(&pool, &store).await.unwrap();
        assert_eq!(cache.default_max_bytes(), DEFAULT_QUOTA_BYTES);
        assert_eq!(cache.max_bytes("nobody@x.org"), DEFAULT_QUOTA_BYTES);
    }

    #[tokio::test]
    async fn test_global_default_db_overrides_config() {
        let pool = init_memory_db().await.unwrap();
        db_execute!(
            pool,
            "INSERT INTO quotas (username, max_storage, created_at, first_login_at, last_login_at)
             VALUES ('__GLOBAL_DEFAULT__', 2000, 0, 0, 0)"
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(dir.path());
        let cache = QuotaCache::new(DEFAULT_QUOTA_BYTES);
        cache.hydrate(&pool, &store).await.unwrap();
        assert_eq!(cache.default_max_bytes(), 2000);
    }

    #[tokio::test]
    async fn test_per_user_zero_uses_default() {
        let pool = init_memory_db().await.unwrap();
        db_execute!(
            pool,
            "INSERT INTO quotas (username, max_storage, created_at, first_login_at, last_login_at)
             VALUES ('bob@x.org', 0, 0, 0, 0)"
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(dir.path());
        let cache = QuotaCache::new(8000);
        cache.hydrate(&pool, &store).await.unwrap();
        let (_, max, is_default) = cache.get_quota("bob@x.org");
        assert_eq!(max, 8000);
        assert!(is_default);
    }

    #[test]
    fn test_quota_record_write_concurrent() {
        let cache = QuotaCache::new(1024 * 1024);
        cache.entries.insert(
            "u@example.org".into(),
            QuotaEntry::new(0, 1024 * 1024, false),
        );
        cache.record_write("u@example.org", 100);
        cache.record_write("u@example.org", 50);
        assert_eq!(cache.used_bytes("u@example.org"), 150);
    }
}
