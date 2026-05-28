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
    db_execute!(
        pool,
        "INSERT INTO blocked_users (username, reason) VALUES (?, ?)
         ON CONFLICT(username) DO UPDATE SET reason = excluded.reason",
        username,
        reason
    )?;
    Ok(())
}

pub async fn unblock_user(pool: &DbPool, username: &str) -> Result<()> {
    db_execute!(pool, "DELETE FROM blocked_users WHERE username = ?", username)?;
    Ok(())
}

pub async fn list_blocked_users(pool: &DbPool) -> Result<Vec<(String, String, String)>> {
    let rows: Vec<(String, String, String)> = db_fetch_all!(
        pool,
        (String, String, String),
        "SELECT username, reason, blocked_at FROM blocked_users ORDER BY blocked_at DESC"
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
        passwords::create_user(&pool, "u@x.org", "hash").await.unwrap();
        passwords::delete_user_full(&pool, "u@x.org", ADMIN_DELETE_REASON)
            .await
            .unwrap();
        assert!(is_blocked(&pool, "u@x.org").await.unwrap());
        assert!(passwords::get_user_hash(&pool, "u@x.org").await.unwrap().is_none());
    }
}
