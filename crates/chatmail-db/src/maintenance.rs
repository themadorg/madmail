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

//! Scheduled maintenance helpers (unused accounts, quota rows).

use chatmail_types::Result;

use crate::passwords;
use crate::{db_execute, db_fetch_all, DbPool};

pub async fn list_dormant_accounts(pool: &DbPool, created_before: i64) -> Result<Vec<String>> {
    let qt = crate::schema::quota_table(pool).await?;
    let sql = format!(
        "SELECT username FROM {qt}
         WHERE first_login_at = 1 AND created_at < ?"
    );
    let rows: Vec<(String,)> = db_fetch_all!(pool, (String,), &sql, created_before)?;
    Ok(rows.into_iter().map(|(u,)| u).collect())
}

pub async fn remove_account_without_blocklist(
    pool: &DbPool,
    username: &str,
    maildir_root: &std::path::Path,
) -> Result<()> {
    if maildir_root.exists() {
        tokio::fs::remove_dir_all(maildir_root).await?;
    }
    passwords::delete_user(pool, username).await?;
    let qt = crate::schema::quota_table(pool).await?;
    let sql = format!("DELETE FROM {qt} WHERE username = ?");
    db_execute!(pool, &sql, username)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{init_memory_db, passwords};

    #[tokio::test]
    async fn dormant_accounts_never_logged_in() {
        let pool = init_memory_db().await.unwrap();
        let now = 1_700_000_000_i64;
        let DbPool::Sqlite(p) = &pool else {
            panic!("memory db is sqlite");
        };
        sqlx::query(
            "INSERT INTO quotas (username, max_storage, created_at, first_login_at, last_login_at)
             VALUES ('old@x.org', 0, 100, 1, 0),
                    ('new@x.org', 0, ?, 1, 0),
                    ('active@x.org', 0, 100, ?, 0)",
        )
        .bind(now)
        .bind(now)
        .execute(p)
        .await
        .unwrap();

        let dormant = list_dormant_accounts(&pool, now - 1).await.unwrap();
        assert_eq!(dormant, vec!["old@x.org".to_string()]);
    }

    #[tokio::test]
    async fn remove_account_without_blocklist_clears_db() {
        let pool = init_memory_db().await.unwrap();
        passwords::create_user(&pool, "gone@x.org", "hash").await.unwrap();
        let DbPool::Sqlite(p) = &pool else {
            panic!("memory db is sqlite");
        };
        sqlx::query(
            "INSERT INTO quotas (username, max_storage, created_at, first_login_at, last_login_at)
             VALUES ('gone@x.org', 0, 0, 1, 0)",
        )
        .execute(p)
        .await
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let mail_root = dir.path().join("user");
        tokio::fs::create_dir_all(&mail_root).await.unwrap();

        remove_account_without_blocklist(&pool, "gone@x.org", &mail_root)
            .await
            .unwrap();
        assert!(!passwords::user_exists(&pool, "gone@x.org").await.unwrap());
        assert!(!mail_root.exists());
        assert!(!crate::blocklist::is_blocked(&pool, "gone@x.org").await.unwrap());
    }
}
