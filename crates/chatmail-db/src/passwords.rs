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

use chatmail_types::Result;

use crate::pool::DbBackend;
use crate::schema::PasswordsLayout;
use crate::{db_execute, db_fetch_all, db_fetch_optional, DbPool};

async fn detect_schema(pool: &DbPool) -> Result<PasswordsLayout> {
    crate::schema::passwords_layout(pool).await
}

pub async fn user_exists(pool: &DbPool, username: &str) -> Result<bool> {
    Ok(get_user_hash(pool, username).await?.is_some())
}

pub async fn get_user_hash(pool: &DbPool, username: &str) -> Result<Option<String>> {
    match detect_schema(pool).await? {
        PasswordsLayout::ChatmailRs => {
            let row: Option<(String,)> = db_fetch_optional!(
                pool,
                (String,),
                "SELECT hash FROM passwords WHERE username = ?",
                username
            )?;
            Ok(row.map(|(h,)| h))
        }
        PasswordsLayout::MadmailKv => {
            let row: Option<(String,)> = db_fetch_optional!(
                pool,
                (String,),
                "SELECT value FROM passwords WHERE key = ?",
                username
            )?;
            Ok(row.map(|(h,)| h))
        }
        PasswordsLayout::Unknown => Ok(None),
    }
}

pub async fn create_user(pool: &DbPool, username: &str, hash: &str) -> Result<()> {
    match detect_schema(pool).await? {
        PasswordsLayout::MadmailKv => {
            db_execute!(
                pool,
                "INSERT INTO passwords (key, value) VALUES (?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                username,
                hash
            )?;
        }
        _ => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            db_execute!(
                pool,
                "INSERT INTO passwords (username, hash, created_at) VALUES (?, ?, ?)
                 ON CONFLICT(username) DO UPDATE SET hash = excluded.hash",
                username,
                hash,
                now
            )?;
        }
    }
    Ok(())
}

pub async fn delete_user(pool: &DbPool, username: &str) -> Result<()> {
    match detect_schema(pool).await? {
        PasswordsLayout::MadmailKv => {
            db_execute!(pool, "DELETE FROM passwords WHERE key = ?", username)?;
        }
        _ => {
            db_execute!(pool, "DELETE FROM passwords WHERE username = ?", username)?;
        }
    }
    Ok(())
}

pub async fn list_users(pool: &DbPool) -> Result<Vec<String>> {
    Ok(list_all_credentials(pool)
        .await?
        .into_iter()
        .map(|(u, _)| u)
        .collect())
}

/// Bulk-load all account credentials (one schema detect + one query). Used by [`AuthCache`].
pub async fn list_all_credentials(pool: &DbPool) -> Result<Vec<(String, String)>> {
    match detect_schema(pool).await? {
        PasswordsLayout::ChatmailRs => {
            let rows: Vec<(String, String)> = db_fetch_all!(
                pool,
                (String, String),
                "SELECT username, hash FROM passwords ORDER BY username"
            )?;
            Ok(rows)
        }
        PasswordsLayout::MadmailKv => {
            let sql = match pool.backend() {
                DbBackend::Sqlite => {
                    "SELECT key, value FROM passwords WHERE key NOT GLOB '__*__' ORDER BY key"
                }
                DbBackend::Postgres => {
                    "SELECT key, value FROM passwords WHERE key !~ '^__.*__$' ORDER BY key"
                }
            };
            let rows: Vec<(String, String)> = db_fetch_all!(pool, (String, String), sql)?;
            Ok(rows)
        }
        PasswordsLayout::Unknown => Ok(Vec::new()),
    }
}

pub async fn delete_user_full(pool: &DbPool, username: &str, reason: &str) -> Result<()> {
    delete_user(pool, username).await?;
    let qt = crate::schema::quota_table(pool).await?;
    let sql = format!("DELETE FROM {qt} WHERE username = ?");
    db_execute!(pool, &sql, username)?;
    crate::blocklist::block_user(pool, username, reason).await?;
    Ok(())
}

pub use crate::blocklist::is_blocked;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_memory_db;

    #[tokio::test]
    async fn test_passwords_crud_madmail_v2() {
        let pool = init_memory_db().await.unwrap();
        create_user(&pool, "u@example.org", "bcrypt:hash")
            .await
            .unwrap();
        assert_eq!(
            get_user_hash(&pool, "u@example.org")
                .await
                .unwrap()
                .as_deref(),
            Some("bcrypt:hash")
        );
    }

    #[tokio::test]
    async fn test_passwords_madmail_kv_schema() {
        let pool = init_memory_db().await.unwrap();
        let DbPool::Sqlite(p) = &pool else {
            panic!("memory db is sqlite");
        };
        sqlx::query("DROP TABLE passwords")
            .execute(p)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE passwords (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(p)
            .await
            .unwrap();
        create_user(&pool, "u@example.org", "bcrypt:legacy")
            .await
            .unwrap();
        assert_eq!(
            get_user_hash(&pool, "u@example.org")
                .await
                .unwrap()
                .as_deref(),
            Some("bcrypt:legacy")
        );
        sqlx::query("INSERT INTO passwords (key, value) VALUES ('__REGISTRATION_OPEN__', 'true')")
            .execute(p)
            .await
            .unwrap();
        create_user(&pool, "00y4t0i0@[1.1.1.1]", "bcrypt:x")
            .await
            .unwrap();
        let users = list_users(&pool).await.unwrap();
        assert_eq!(users.len(), 2);
        assert!(users.contains(&"u@example.org".to_string()));
        assert!(users.contains(&"00y4t0i0@[1.1.1.1]".to_string()));
        assert!(!users.iter().any(|u| u.starts_with("__")));
    }

    #[tokio::test]
    async fn test_is_blocked() {
        let pool = init_memory_db().await.unwrap();
        crate::blocklist::block_user(&pool, "bad@example.org", "test")
            .await
            .unwrap();
        assert!(is_blocked(&pool, "bad@example.org").await.unwrap());
    }
}
