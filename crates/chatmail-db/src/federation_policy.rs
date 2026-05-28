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

//! Federation policy settings (`__FEDERATION_POLICY__`), Madmail-compatible.

use chatmail_types::Result;
use crate::DbPool;

use crate::settings::{get_setting, set_setting};
use crate::settings_keys::FEDERATION_POLICY;

/// Strip IP brackets and lowercase (Madmail `normalizeDomain`).
pub fn normalize_federation_domain(domain: &str) -> String {
    let d = domain.trim();
    let d = d.strip_prefix('[').unwrap_or(d);
    let d = d.strip_suffix(']').unwrap_or(d);
    d.to_ascii_lowercase()
}

/// Read policy string from DB; default `ACCEPT` when unset (Madmail).
pub async fn federation_policy_label(pool: &DbPool) -> Result<String> {
    let raw = get_setting(pool, FEDERATION_POLICY).await?;
    Ok(match raw {
        Some(v) if !v.trim().is_empty() => v.trim().to_ascii_uppercase(),
        _ => "ACCEPT".into(),
    })
}

pub async fn set_federation_policy_label(pool: &DbPool, policy: &str) -> Result<()> {
    let p = policy.trim().to_ascii_uppercase();
    if p != "ACCEPT" && p != "REJECT" {
        return Err(chatmail_types::ChatmailError::config(format!(
            "invalid federation policy: {policy} (expected ACCEPT or REJECT)"
        )));
    }
    set_setting(pool, FEDERATION_POLICY, &p).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_memory_db;

    #[test]
    fn normalize_strips_ip_brackets() {
        assert_eq!(
            normalize_federation_domain("[1.1.1.1]"),
            "1.1.1.1"
        );
        assert_eq!(
            normalize_federation_domain("Example.ORG"),
            "example.org"
        );
    }

    #[tokio::test]
    async fn policy_defaults_to_accept() {
        let pool = init_memory_db().await.unwrap();
        assert_eq!(federation_policy_label(&pool).await.unwrap(), "ACCEPT");
    }

    #[tokio::test]
    async fn policy_reads_from_passwords_kv_when_settings_empty() {
        let pool = init_memory_db().await.unwrap();
        let DbPool::Sqlite(p) = &pool else {
            panic!("memory db is sqlite");
        };
        sqlx::query("DROP TABLE passwords").execute(p).await.unwrap();
        sqlx::query("CREATE TABLE passwords (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(p)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO passwords (key, value) VALUES ('__FEDERATION_POLICY__', 'REJECT')",
        )
        .execute(p)
        .await
        .unwrap();
        assert_eq!(federation_policy_label(&pool).await.unwrap(), "REJECT");
    }
}
