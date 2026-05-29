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

//! In-memory credentials cache (Madmail Go `pass_table.credCache` parity).
//!
//! Hot paths (routing, SMTP/IMAP/Web auth) must not hit the DB per recipient or
//! per login when the account is already known. Hydrate at boot and on soft reload.

use chatmail_db::{is_federation_rcpt_blocked, passwords, DbPool};
use chatmail_types::Result;
use dashmap::DashMap;

/// Username → stored password hash (`bcrypt:…` or legacy `algo:hash`).
#[derive(Debug)]
pub struct AuthCache {
    entries: DashMap<String, String>,
}

impl AuthCache {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    /// O(1) existence check; no allocation.
    pub fn user_exists(&self, username: &str) -> bool {
        self.entries.contains_key(username)
    }

    pub fn get_hash(&self, username: &str) -> Option<String> {
        self.entries.get(username).map(|v| v.clone())
    }

    /// Write-through after DB insert/update (JIT, admin API, import).
    pub fn insert(&self, username: impl Into<String>, hash: impl Into<String>) {
        self.entries.insert(username.into(), hash.into());
    }

    pub fn remove(&self, username: &str) {
        self.entries.remove(username);
    }

    /// Whether inbound mail may be delivered locally (reserved rcpt + account exists).
    pub fn local_recipient_allowed(&self, rcpt: &str) -> bool {
        if is_federation_rcpt_blocked(rcpt) {
            return false;
        }
        self.user_exists(rcpt)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Full reload from DB (boot, SIGUSR2 / admin soft reload).
    pub async fn hydrate(&self, pool: &DbPool) -> Result<()> {
        let rows = passwords::list_all_credentials(pool).await?;
        self.entries.clear();
        for (user, hash) in rows {
            self.entries.insert(user, hash);
        }
        Ok(())
    }
}

impl Default for AuthCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::{init_memory_db, passwords};

    #[tokio::test]
    async fn hydrate_loads_all_users() {
        let pool = init_memory_db().await.unwrap();
        passwords::create_user(&pool, "a@test", "bcrypt:1")
            .await
            .unwrap();
        passwords::create_user(&pool, "b@test", "bcrypt:2")
            .await
            .unwrap();

        let cache = AuthCache::new();
        cache.hydrate(&pool).await.unwrap();
        assert_eq!(cache.len(), 2);
        assert!(cache.user_exists("a@test"));
        assert_eq!(cache.get_hash("b@test").as_deref(), Some("bcrypt:2"));
    }

    #[test]
    fn local_recipient_blocks_reserved_and_unknown() {
        let cache = AuthCache::new();
        cache.insert("u@test", "h");
        assert!(!cache.local_recipient_allowed("admin@test"));
        assert!(!cache.local_recipient_allowed("ghost@test"));
        assert!(cache.local_recipient_allowed("u@test"));
    }

    #[test]
    fn insert_remove_roundtrip() {
        let cache = AuthCache::new();
        cache.insert("u@test", "bcrypt:x");
        assert!(cache.user_exists("u@test"));
        cache.remove("u@test");
        assert!(!cache.user_exists("u@test"));
    }
}
