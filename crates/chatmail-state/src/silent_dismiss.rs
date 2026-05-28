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

use std::collections::HashSet;
use std::sync::RwLock;

use chatmail_db::{
    db_execute, db_fetch_all, normalize_federation_domain, pg_sql, DbPool,
};
use chatmail_types::{address_domain, address_is_local, domain_forms, Result};

#[derive(Debug)]
pub struct FederationSilentDismissCache {
    domains: RwLock<HashSet<String>>,
}

impl FederationSilentDismissCache {
    pub fn new() -> Self {
        Self {
            domains: RwLock::new(HashSet::new()),
        }
    }

    pub async fn hydrate(&self, pool: &DbPool) -> Result<()> {
        let rows: Vec<(String,)> =
            db_fetch_all!(pool, (String,), "SELECT domain FROM federation_silent_dismiss")?;
        let mut set = self.domains.write().expect("silent dismiss lock");
        set.clear();
        for (domain,) in rows {
            set.insert(normalize_federation_domain(&domain));
        }
        Ok(())
    }

    pub fn is_dismissed(&self, rcpt: &str, local_domains: &[String]) -> bool {
        if address_is_local(rcpt, local_domains) {
            return false;
        }
        let Some(domain) = address_domain(rcpt) else {
            return false;
        };
        let set = self.domains.read().expect("silent dismiss lock");
        domain_forms(&domain)
            .iter()
            .map(|f| normalize_federation_domain(f))
            .any(|form| set.contains(&form))
    }

    pub fn list_domains(&self) -> Vec<String> {
        let mut domains: Vec<String> = self
            .domains
            .read()
            .expect("silent dismiss lock")
            .iter()
            .cloned()
            .collect();
        domains.sort();
        domains
    }

    pub async fn add(&self, pool: &DbPool, domain: &str) -> Result<()> {
        let domain = normalize_federation_domain(domain);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        db_execute!(
            pool,
            "INSERT INTO federation_silent_dismiss (domain, created_at) VALUES (?, ?)
             ON CONFLICT(domain) DO NOTHING",
            domain.as_str(),
            now
        )?;
        self.domains
            .write()
            .expect("silent dismiss lock")
            .insert(domain);
        Ok(())
    }

    pub async fn remove(&self, pool: &DbPool, domain: &str) -> Result<bool> {
        let domain = normalize_federation_domain(domain);
        let removed_db = match pool {
            DbPool::Sqlite(p) => {
                sqlx::query("DELETE FROM federation_silent_dismiss WHERE domain = ?")
                    .bind(&domain)
                    .execute(p)
                    .await?
                    .rows_affected()
                    > 0
            }
            DbPool::Postgres(p) => {
                sqlx::query(&pg_sql(
                    "DELETE FROM federation_silent_dismiss WHERE domain = ?",
                ))
                .bind(&domain)
                .execute(p)
                .await?
                .rows_affected()
                    > 0
            }
        };
        let removed = self
            .domains
            .write()
            .expect("silent dismiss lock")
            .remove(&domain);
        Ok(removed || removed_db)
    }

    pub async fn list_rules(&self, pool: &DbPool) -> Result<Vec<(String, i64)>> {
        let rows: Vec<(String, i64)> = db_fetch_all!(
            pool,
            (String, i64),
            "SELECT domain, created_at FROM federation_silent_dismiss ORDER BY domain"
        )?;
        Ok(rows)
    }

    pub async fn flush(&self, pool: &DbPool) -> Result<()> {
        db_execute!(pool, "DELETE FROM federation_silent_dismiss")?;
        self.domains.write().expect("silent dismiss lock").clear();
        Ok(())
    }

    pub async fn add_count(&self, pool: &DbPool, domain: &str) -> Result<usize> {
        self.add(pool, domain).await?;
        Ok(self.list_domains().len())
    }

    pub async fn remove_count(&self, pool: &DbPool, domain: &str) -> Result<usize> {
        self.remove(pool, domain).await?;
        Ok(self.list_domains().len())
    }
}

impl Default for FederationSilentDismissCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::init_memory_db;

    #[tokio::test]
    async fn dismiss_matches_ip_bracket_and_bare() {
        let pool = init_memory_db().await.unwrap();
        let cache = FederationSilentDismissCache::new();
        cache.hydrate(&pool).await.unwrap();
        cache.add(&pool, "[1.1.1.1]").await.unwrap();

        let local = chatmail_types::build_local_domains("local.test", None);
        assert!(cache.is_dismissed("user@[1.1.1.1]", &local));
        assert!(cache.is_dismissed("user@1.1.1.1", &local));
        assert!(!cache.is_dismissed("user@local.test", &local));
    }

    #[tokio::test]
    async fn dismiss_matches_domain_case_insensitive() {
        let pool = init_memory_db().await.unwrap();
        let cache = FederationSilentDismissCache::new();
        cache.add(&pool, "Evil.Example").await.unwrap();

        let local = vec!["local.test".into()];
        assert!(cache.is_dismissed("u@evil.example", &local));
    }
}
