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
use chatmail_types::Result;

/// Global federation mode (TDD / Madmail admin API).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyMode {
    /// Default allow; `exceptions` is a blocklist.
    Accept,
    /// Default deny; `exceptions` is an allowlist.
    Reject,
}

impl PolicyMode {
    pub fn from_label(label: &str) -> Self {
        if label.eq_ignore_ascii_case("reject") {
            Self::Reject
        } else {
            Self::Accept
        }
    }
}

#[derive(Debug)]
pub struct FederationPolicyCache {
    exceptions: RwLock<HashSet<String>>,
}

impl FederationPolicyCache {
    pub fn new() -> Self {
        Self {
            exceptions: RwLock::new(HashSet::new()),
        }
    }

    pub async fn hydrate(&self, pool: &DbPool) -> Result<()> {
        let rows: Vec<(String,)> =
            db_fetch_all!(pool, (String,), "SELECT domain FROM federation_rules")?;
        let mut set = self
            .exceptions
            .write()
            .expect("policy cache lock");
        set.clear();
        for (domain,) in rows {
            set.insert(normalize_federation_domain(&domain));
        }
        Ok(())
    }

    /// Returns `true` if delivery from `domain` is allowed under `global_policy`.
    pub fn check_policy(&self, domain: &str, global_policy: PolicyMode) -> bool {
        self.check_policy_normalized(&normalize_federation_domain(domain), global_policy)
    }

    /// Madmail `CheckFederationPolicy` with normalized domain.
    pub fn check_policy_normalized(&self, domain: &str, global_policy: PolicyMode) -> bool {
        let exceptions = self.exceptions.read().expect("policy cache lock");
        let listed = exceptions.contains(domain);
        match global_policy {
            PolicyMode::Accept => !listed,
            PolicyMode::Reject => listed,
        }
    }

    /// Local domains always bypass federation policy (Madmail).
    pub fn allows_sender(
        &self,
        sender_domain: &str,
        local_domains: &[String],
        global_policy: PolicyMode,
    ) -> bool {
        let sender = normalize_federation_domain(sender_domain);
        if sender.is_empty() {
            return true;
        }
        if local_domains
            .iter()
            .any(|local| normalize_federation_domain(local) == sender)
        {
            return true;
        }
        self.check_policy_normalized(&sender, global_policy)
    }

    pub fn add_exception(&self, domain: &str) {
        self.exceptions
            .write()
            .expect("policy cache lock")
            .insert(normalize_federation_domain(domain));
    }

    pub fn remove_exception(&self, domain: &str) -> bool {
        self.exceptions
            .write()
            .expect("policy cache lock")
            .remove(&normalize_federation_domain(domain))
    }

    pub fn list_exceptions(&self) -> Vec<String> {
        let mut domains: Vec<String> = self
            .exceptions
            .read()
            .expect("policy cache lock")
            .iter()
            .cloned()
            .collect();
        domains.sort();
        domains
    }

    pub async fn add_rule(&self, pool: &DbPool, domain: &str) -> Result<()> {
        let domain = normalize_federation_domain(domain);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        db_execute!(
            pool,
            "INSERT INTO federation_rules (domain, created_at) VALUES (?, ?)
             ON CONFLICT(domain) DO NOTHING",
            domain.as_str(),
            now
        )?;
        self.add_exception(&domain);
        Ok(())
    }

    pub async fn remove_rule(&self, pool: &DbPool, domain: &str) -> Result<bool> {
        let domain = normalize_federation_domain(domain);
        let removed_db = match pool {
            DbPool::Sqlite(p) => {
                sqlx::query("DELETE FROM federation_rules WHERE domain = ?")
                    .bind(&domain)
                    .execute(p)
                    .await?
                    .rows_affected()
                    > 0
            }
            DbPool::Postgres(p) => {
                sqlx::query(&pg_sql("DELETE FROM federation_rules WHERE domain = ?"))
                    .bind(&domain)
                    .execute(p)
                    .await?
                    .rows_affected()
                    > 0
            }
        };
        let removed = self.remove_exception(&domain);
        Ok(removed || removed_db)
    }

    pub async fn list_rules(&self, pool: &DbPool) -> Result<Vec<(String, i64)>> {
        let rows: Vec<(String, i64)> = db_fetch_all!(
            pool,
            (String, i64),
            "SELECT domain, created_at FROM federation_rules ORDER BY domain"
        )?;
        Ok(rows)
    }

    /// Remove all federation domain exceptions (Madmail `FlushRules`).
    pub async fn flush_rules(&self, pool: &DbPool) -> Result<()> {
        db_execute!(pool, "DELETE FROM federation_rules")?;
        self.exceptions.write().expect("policy cache lock").clear();
        Ok(())
    }

    /// Add rule and return total exception count (Madmail `AddRule`).
    pub async fn add_rule_count(&self, pool: &DbPool, domain: &str) -> Result<usize> {
        self.add_rule(pool, domain).await?;
        Ok(self.list_exceptions().len())
    }

    /// Remove rule and return remaining count (Madmail `RemoveRule`).
    pub async fn remove_rule_count(&self, pool: &DbPool, domain: &str) -> Result<usize> {
        self.remove_rule(pool, domain).await?;
        Ok(self.list_exceptions().len())
    }
}

impl Default for FederationPolicyCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P2-UT05: ACCEPT blocklist and REJECT allowlist.
    #[test]
    fn p2_ut05_test_policy_evaluator() {
        let cache = FederationPolicyCache::new();
        cache.add_exception("spam.example.org");

        assert!(cache.check_policy("good.example.org", PolicyMode::Accept));
        assert!(!cache.check_policy("spam.example.org", PolicyMode::Accept));

        assert!(!cache.check_policy("good.example.org", PolicyMode::Reject));
        assert!(cache.check_policy("spam.example.org", PolicyMode::Reject));
    }

    /// TDD 16-testing: policy domain checks are case-insensitive.
    #[test]
    fn test_policy_domain_case_insensitive() {
        let cache = FederationPolicyCache::new();
        cache.add_exception("Spam.Example.ORG");
        assert!(!cache.check_policy("spam.example.org", PolicyMode::Accept));
    }
}
