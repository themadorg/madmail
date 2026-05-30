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

//! Registration invite tokens (`registration_tokens` table).

use chatmail_types::{ChatmailError, Result};

use crate::pool::pg_sql;
use crate::{db_execute, db_fetch_one, db_fetch_optional, DbPool};

pub fn token_not_found() -> ChatmailError {
    ChatmailError::config("registration token not found")
}

pub fn token_expired() -> ChatmailError {
    ChatmailError::config("registration token has expired")
}

pub fn token_exhausted() -> ChatmailError {
    ChatmailError::config("registration token has been fully used")
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn fetch_scalar_i64(pool: &DbPool, sql: &str, bind: &str) -> Result<i64> {
    let row: (i64,) = db_fetch_one!(pool, (i64,), sql, bind)?;
    Ok(row.0)
}

pub async fn validate_registration_token(pool: &DbPool, token: &str) -> Result<()> {
    let token = token.trim();
    if token.is_empty() {
        return Err(token_not_found());
    }

    let row: Option<(i64, i64)> = db_fetch_optional!(
        pool,
        (i64, i64),
        "SELECT max_uses, used_count FROM registration_tokens
         WHERE token = ?
           AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)",
        token
    )?;

    let Some((max_uses, used_count)) = row else {
        let exists: Option<(Option<String>,)> = db_fetch_optional!(
            pool,
            (Option<String>,),
            "SELECT expires_at FROM registration_tokens WHERE token = ?",
            token
        )?;
        return Err(if exists.is_some() {
            token_expired()
        } else {
            token_not_found()
        });
    };

    if used_count >= max_uses {
        return Err(token_exhausted());
    }

    let pending = {
        let qt = crate::schema::quota_table(pool).await?;
        let sql = format!(
            "SELECT COUNT(*) FROM {qt}
         WHERE used_token = ? AND first_login_at = 1"
        );
        fetch_scalar_i64(pool, &sql, token).await?
    };

    if used_count + pending >= max_uses {
        return Err(token_exhausted());
    }

    Ok(())
}

pub async fn ensure_new_account_quota(pool: &DbPool, username: &str) -> Result<()> {
    let now = unix_now();
    let qt = crate::schema::quota_table(pool).await?;
    let sql = format!(
        "INSERT INTO {qt} (username, max_storage, created_at, first_login_at, last_login_at, used_token)
         VALUES (?, 0, ?, 1, 0, NULL)
         ON CONFLICT(username) DO UPDATE SET
             created_at = CASE WHEN {qt}.created_at = 0 THEN excluded.created_at ELSE {qt}.created_at END,
             first_login_at = CASE WHEN {qt}.first_login_at = 0 THEN 1 ELSE {qt}.first_login_at END"
    );
    db_execute!(pool, &sql, username, now)?;
    Ok(())
}

pub async fn attach_registration_token(pool: &DbPool, username: &str, token: &str) -> Result<()> {
    ensure_new_account_quota(pool, username).await?;
    let qt = crate::schema::quota_table(pool).await?;
    let sql = format!("UPDATE {qt} SET used_token = ? WHERE username = ?");
    db_execute!(pool, &sql, token.trim(), username)?;
    Ok(())
}

pub async fn reserve_registration_token(pool: &DbPool, username: &str, token: &str) -> Result<()> {
    attach_registration_token(pool, username, token).await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstLoginOutcome {
    Ok,
    AccountRemoved,
}

/// Usernames whose first-login quota bookkeeping is complete (`first_login_at != 1`).
pub async fn list_login_settled_usernames(pool: &DbPool) -> Result<Vec<String>> {
    let qt = crate::schema::quota_table(pool).await?;
    let sql = format!("SELECT username FROM {qt} WHERE first_login_at != 1");
    let rows: Vec<(String,)> = crate::db_fetch_all!(pool, (String,), &sql)?;
    Ok(rows.into_iter().map(|(u,)| u).collect())
}

pub async fn record_first_login(pool: &DbPool, username: &str) -> Result<FirstLoginOutcome> {
    let qt = crate::schema::quota_table(pool).await?;
    let select_sql = format!("SELECT first_login_at, used_token FROM {qt} WHERE username = ?");
    let row: Option<(i64, Option<String>)> =
        db_fetch_optional!(pool, (i64, Option<String>), &select_sql, username)?;

    let Some((first_login_at, used_token)) = row else {
        return Ok(FirstLoginOutcome::Ok);
    };

    let now = unix_now();

    if first_login_at != 1 {
        let sql = format!("UPDATE {qt} SET last_login_at = ? WHERE username = ?");
        db_execute!(pool, &sql, now, username)?;
        return Ok(FirstLoginOutcome::Ok);
    }

    if let Some(ref token) = used_token {
        if !token.is_empty() {
            let consumed = consume_registration_token(pool, token).await?;
            if !consumed {
                let sql = format!("DELETE FROM {qt} WHERE username = ?");
                db_execute!(pool, &sql, username)?;
                return Ok(FirstLoginOutcome::AccountRemoved);
            }
        }
    }

    let sql = format!(
        "UPDATE {qt} SET first_login_at = ?, last_login_at = ?, used_token = NULL WHERE username = ?"
    );
    db_execute!(pool, &sql, now, now, username)?;

    Ok(FirstLoginOutcome::Ok)
}

async fn consume_registration_token(pool: &DbPool, token: &str) -> Result<bool> {
    let affected = match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE registration_tokens SET used_count = used_count + 1
                 WHERE token = ? AND used_count < max_uses",
        )
        .bind(token.trim())
        .execute(p)
        .await?
        .rows_affected(),
        DbPool::Postgres(p) => sqlx::query(&pg_sql(
            "UPDATE registration_tokens SET used_count = used_count + 1
                 WHERE token = ? AND used_count < max_uses",
        ))
        .bind(token.trim())
        .execute(p)
        .await?
        .rows_affected(),
    };
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_memory_db;

    async fn seed_token(pool: &DbPool, token: &str, max_uses: i32) {
        db_execute!(
            pool,
            "INSERT INTO registration_tokens (token, max_uses, used_count, comment)
             VALUES (?, ?, 0, '')",
            token,
            max_uses
        )
        .unwrap();
    }

    #[tokio::test]
    async fn signup_with_token_shows_pending_reservation() {
        let pool = init_memory_db().await.unwrap();
        seed_token(&pool, "invite-abc", 2).await;

        validate_registration_token(&pool, "invite-abc")
            .await
            .unwrap();
        ensure_new_account_quota(&pool, "alice@x.org")
            .await
            .unwrap();
        attach_registration_token(&pool, "alice@x.org", "invite-abc")
            .await
            .unwrap();

        let pending = fetch_scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM quotas WHERE used_token = ? AND first_login_at = 1",
            "invite-abc",
        )
        .await
        .unwrap();
        assert_eq!(pending, 1);

        let used_count: (i32,) = db_fetch_one!(
            &pool,
            (i32,),
            "SELECT used_count FROM registration_tokens WHERE token = ?",
            "invite-abc"
        )
        .unwrap();
        assert_eq!(used_count.0, 0);
    }

    #[tokio::test]
    async fn first_login_consumes_token_and_clears_pending() {
        let pool = init_memory_db().await.unwrap();
        seed_token(&pool, "invite-xyz", 1).await;
        attach_registration_token(&pool, "bob@x.org", "invite-xyz")
            .await
            .unwrap();

        assert_eq!(
            record_first_login(&pool, "bob@x.org").await.unwrap(),
            FirstLoginOutcome::Ok
        );

        let used_count: (i32,) = db_fetch_one!(
            &pool,
            (i32,),
            "SELECT used_count FROM registration_tokens WHERE token = ?",
            "invite-xyz"
        )
        .unwrap();
        assert_eq!(used_count.0, 1);

        let pending = fetch_scalar_i64(
            &pool,
            "SELECT COUNT(*) FROM quotas WHERE used_token = ? AND first_login_at = 1",
            "invite-xyz",
        )
        .await
        .unwrap();
        assert_eq!(pending, 0);

        let used_token: (Option<String>,) = db_fetch_one!(
            &pool,
            (Option<String>,),
            "SELECT used_token FROM quotas WHERE username = ?",
            "bob@x.org"
        )
        .unwrap();
        assert!(used_token.0.is_none());
    }

    #[tokio::test]
    async fn first_login_with_invalid_token_removes_quota() {
        let pool = init_memory_db().await.unwrap();
        let DbPool::Sqlite(p) = &pool else {
            panic!("memory db is sqlite");
        };
        sqlx::query(
            "INSERT INTO quotas (username, max_storage, created_at, first_login_at, last_login_at, used_token)
             VALUES ('gone@x.org', 0, 1, 1, 0, 'deleted-token')",
        )
        .execute(p)
        .await
        .unwrap();

        assert_eq!(
            record_first_login(&pool, "gone@x.org").await.unwrap(),
            FirstLoginOutcome::AccountRemoved
        );

        let exists: Option<(String,)> = db_fetch_optional!(
            &pool,
            (String,),
            "SELECT username FROM quotas WHERE username = ?",
            "gone@x.org"
        )
        .unwrap();
        assert!(exists.is_none());
    }
}
