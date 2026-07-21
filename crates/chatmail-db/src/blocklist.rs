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

//! Blocklist (`blocked_users` table) — Madmail `storage.imapsql` `BlockUser` / `IsBlocked`.

use chatmail_types::Result;

use crate::{db_execute, db_fetch_all, db_fetch_optional, DbPool};

pub const ADMIN_DELETE_REASON: &str = "deleted via admin panel";
pub const BULK_DELETE_REASON: &str = "bulk delete via admin";
pub const MANUAL_BLOCK_REASON: &str = "manually blocked";
pub const CLI_DELETE_REASON: &str = "account deleted via CLI";
pub const CLI_BAN_REASON: &str = "banned via CLI";

pub async fn block_user(pool: &DbPool, username: &str, reason: &str) -> Result<()> {
    // Set blocked_at explicitly so legacy Postgres tables without a DEFAULT still get a value.
    db_execute!(
        pool,
        "INSERT INTO blocked_users (username, reason, blocked_at) VALUES (?, ?, CURRENT_TIMESTAMP)
         ON CONFLICT(username) DO UPDATE SET reason = excluded.reason",
        username,
        reason
    )?;
    Ok(())
}

pub async fn unblock_user(pool: &DbPool, username: &str) -> Result<()> {
    db_execute!(
        pool,
        "DELETE FROM blocked_users WHERE username = ?",
        username
    )?;
    Ok(())
}

pub async fn list_blocked_users(pool: &DbPool) -> Result<Vec<(String, String, String)>> {
    // CAST to TEXT: Postgres stores blocked_at as TIMESTAMP; SQLx cannot decode that as String.
    // COALESCE: legacy/null rows must not abort AuthCache hydrate (boot-fatal on Postgres).
    let rows: Vec<(String, String, String)> = db_fetch_all!(
        pool,
        (String, String, String),
        "SELECT username, reason, COALESCE(CAST(blocked_at AS TEXT), '')
         FROM blocked_users ORDER BY blocked_at DESC"
    )?;
    Ok(rows)
}

pub async fn is_blocked(pool: &DbPool, username: &str) -> Result<bool> {
    let row: Option<(i32,)> = db_fetch_optional!(
        pool,
        (i32,),
        "SELECT 1 FROM blocked_users WHERE username = ? LIMIT 1",
        username
    )?;
    Ok(row.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_memory_db;
    use crate::passwords;
    use crate::DbPool;

    #[tokio::test]
    async fn block_prevents_reregistration_check() {
        let pool = init_memory_db().await.unwrap();
        block_user(&pool, "gone@x.org", ADMIN_DELETE_REASON)
            .await
            .unwrap();
        assert!(is_blocked(&pool, "gone@x.org").await.unwrap());
    }

    #[tokio::test]
    async fn unblock_allows_reregistration_check() {
        let pool = init_memory_db().await.unwrap();
        block_user(&pool, "gone@x.org", "spam").await.unwrap();
        unblock_user(&pool, "gone@x.org").await.unwrap();
        assert!(!is_blocked(&pool, "gone@x.org").await.unwrap());
    }

    #[tokio::test]
    async fn delete_user_full_blocks() {
        let pool = init_memory_db().await.unwrap();
        passwords::create_user(&pool, "u@x.org", "hash")
            .await
            .unwrap();
        passwords::delete_user_full(&pool, "u@x.org", ADMIN_DELETE_REASON)
            .await
            .unwrap();
        assert!(is_blocked(&pool, "u@x.org").await.unwrap());
        assert!(passwords::get_user_hash(&pool, "u@x.org")
            .await
            .unwrap()
            .is_none());
    }

    /// list_blocked_users must return a string timestamp (admin API / ban-list CLI).
    #[tokio::test]
    async fn list_blocked_users_returns_blocked_at() {
        let pool = init_memory_db().await.unwrap();
        block_user(&pool, "u@x.org", "test").await.unwrap();
        let rows = list_blocked_users(&pool).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "u@x.org");
        assert_eq!(rows[0].1, "test");
        assert!(
            !rows[0].2.is_empty(),
            "blocked_at should be non-empty, got {:?}",
            rows[0].2
        );
    }

    /// NULL blocked_at must not fail decode (legacy Postgres / issue #97).
    #[tokio::test]
    async fn list_blocked_users_tolerates_null_blocked_at() {
        let pool = init_memory_db().await.unwrap();
        let DbPool::Sqlite(p) = &pool else {
            panic!("memory db is sqlite");
        };
        sqlx::query("INSERT INTO blocked_users (username, reason, blocked_at) VALUES (?, ?, NULL)")
            .bind("null@x.org")
            .bind("legacy")
            .execute(p)
            .await
            .unwrap();

        let rows = list_blocked_users(&pool).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "null@x.org");
        assert_eq!(rows[0].1, "legacy");
        assert_eq!(rows[0].2, "");
    }

    /// Postgres e2e for issue #97. Opt-in via MADMAIL_TEST_PG_DSN.
    #[tokio::test]
    async fn postgres_list_blocked_users_after_block() {
        let Some(dsn) = std::env::var("MADMAIL_TEST_PG_DSN")
            .ok()
            .filter(|s| !s.is_empty())
        else {
            eprintln!("skip: set MADMAIL_TEST_PG_DSN for Postgres e2e");
            return;
        };
        let config = chatmail_config::DatabaseConfig {
            driver: chatmail_config::DbDriver::Postgres,
            dsn,
        };
        let pool = crate::connect_database(&config)
            .await
            .expect("connect postgres");
        crate::pool::run_migrations(&pool)
            .await
            .expect("migrate postgres");

        let user = format!(
            "issue97-{}@test",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        block_user(&pool, &user, ADMIN_DELETE_REASON)
            .await
            .expect("block_user");
        let rows = list_blocked_users(&pool)
            .await
            .expect("list_blocked_users must decode TIMESTAMP as String on Postgres");
        let hit = rows.iter().find(|(u, _, _)| u == &user);
        let Some((_, reason, blocked_at)) = hit else {
            panic!("blocked user not listed");
        };
        assert_eq!(reason, ADMIN_DELETE_REASON);
        assert!(
            !blocked_at.is_empty(),
            "blocked_at empty on postgres: {blocked_at:?}"
        );

        // Legacy NULL blocked_at must not break list (boot hydrate path).
        match &pool {
            DbPool::Postgres(p) => {
                let null_user = format!("{user}-null");
                sqlx::query(
                    "INSERT INTO blocked_users (username, reason, blocked_at)
                     VALUES ($1, $2, NULL)
                     ON CONFLICT (username) DO UPDATE SET blocked_at = NULL",
                )
                .bind(&null_user)
                .bind("null-legacy")
                .execute(p)
                .await
                .unwrap();
            }
            _ => unreachable!(),
        }
        let rows = list_blocked_users(&pool)
            .await
            .expect("NULL blocked_at must not fail list on Postgres");
        assert!(rows
            .iter()
            .any(|(u, r, at)| { u.ends_with("-null") && r == "null-legacy" && at.is_empty() }));

        // Cleanup test rows.
        let _ = unblock_user(&pool, &user).await;
        let _ = unblock_user(&pool, &format!("{user}-null")).await;
    }
}
