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

//! Per-account metadata from `quotas` (Madmail `GetAllAccountInfo`).

use std::collections::HashMap;

use chatmail_types::Result;

use crate::settings_keys::GLOBAL_QUOTA_USERNAME;
use crate::{db_execute, db_fetch_all, DbPool};

#[derive(Debug, Clone, Copy, Default)]
pub struct AccountQuotaInfo {
    pub created_at: i64,
    pub first_login_at: i64,
    pub last_login_at: i64,
}

pub async fn list_account_quota_info(pool: &DbPool) -> Result<HashMap<String, AccountQuotaInfo>> {
    let qt = crate::schema::quota_table(pool).await?;
    let sql = format!(
        "SELECT username, created_at, first_login_at, last_login_at FROM {qt}
         WHERE username != ?"
    );
    let rows: Vec<(String, i64, i64, i64)> = db_fetch_all!(pool, (String, i64, i64, i64), &sql, GLOBAL_QUOTA_USERNAME)?;

    Ok(rows
        .into_iter()
        .map(|(u, created_at, first_login_at, last_login_at)| {
            (
                u,
                AccountQuotaInfo {
                    created_at,
                    first_login_at,
                    last_login_at,
                },
            )
        })
        .collect())
}

pub async fn delete_quota_row(pool: &DbPool, username: &str) -> Result<()> {
    let qt = crate::schema::quota_table(pool).await?;
    let sql = format!("DELETE FROM {qt} WHERE username = ?");
    db_execute!(pool, &sql, username)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_memory_db;

    #[tokio::test]
    async fn list_account_quota_info_excludes_global_default() {
        let pool = init_memory_db().await.unwrap();
        let DbPool::Sqlite(p) = &pool else {
            panic!("memory db is sqlite");
        };
        sqlx::query(
            "INSERT INTO quotas (username, max_storage, created_at, first_login_at, last_login_at)
             VALUES ('alice@x.org', 0, 100, 1, 0)",
        )
        .execute(p)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO quotas (username, max_storage, created_at, first_login_at, last_login_at)
             VALUES (?, 1024, 0, 0, 0)",
        )
        .bind(GLOBAL_QUOTA_USERNAME)
        .execute(p)
        .await
        .unwrap();

        let map = list_account_quota_info(&pool).await.unwrap();
        assert_eq!(map.len(), 1);
        let info = map.get("alice@x.org").unwrap();
        assert_eq!(info.created_at, 100);
        assert_eq!(info.first_login_at, 1);
    }
}
