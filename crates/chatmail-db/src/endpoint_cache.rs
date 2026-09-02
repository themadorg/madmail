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

//! Endpoint override cache (`dns_overrides` table, Madmail `EndpointOverride`).

use chatmail_types::{ChatmailError, Result};

use crate::pool::pg_sql;
use crate::{db_execute, db_fetch_all, db_fetch_optional, DbPool};

type EndpointOverrideTuple = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

#[derive(Debug, Clone)]
pub struct EndpointOverrideRow {
    pub lookup_key: String,
    pub target_host: String,
    pub comment: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

const SELECT_ALL: &str =
    "SELECT lookup_key, target_host, comment, CAST(created_at AS TEXT), CAST(updated_at AS TEXT)
         FROM dns_overrides ORDER BY created_at DESC";

const SELECT_ONE: &str =
    "SELECT lookup_key, target_host, comment, CAST(created_at AS TEXT), CAST(updated_at AS TEXT)
         FROM dns_overrides WHERE lookup_key = ?";

fn map_row(
    (lookup_key, target_host, comment, created_at, updated_at): (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
) -> EndpointOverrideRow {
    EndpointOverrideRow {
        lookup_key,
        target_host,
        comment,
        created_at,
        updated_at,
    }
}

pub async fn list_endpoint_overrides(pool: &DbPool) -> Result<Vec<EndpointOverrideRow>> {
    let rows: Vec<EndpointOverrideTuple> = db_fetch_all!(pool, EndpointOverrideTuple, SELECT_ALL)?;
    Ok(rows.into_iter().map(map_row).collect())
}

pub async fn get_endpoint_override(
    pool: &DbPool,
    lookup_key: &str,
) -> Result<Option<EndpointOverrideRow>> {
    let row: Option<EndpointOverrideTuple> =
        db_fetch_optional!(pool, EndpointOverrideTuple, SELECT_ONE, lookup_key)?;
    Ok(row.map(map_row))
}

pub async fn set_endpoint_override(
    pool: &DbPool,
    lookup_key: &str,
    target_host: &str,
    comment: &str,
) -> Result<()> {
    if lookup_key.trim().is_empty() || target_host.trim().is_empty() {
        return Err(ChatmailError::config(
            "LOOKUP_KEY and TARGET_HOST are required",
        ));
    }
    db_execute!(
        pool,
        "INSERT INTO dns_overrides (lookup_key, target_host, comment)
         VALUES (?, ?, ?)
         ON CONFLICT(lookup_key) DO UPDATE SET
           target_host = excluded.target_host,
           comment = excluded.comment",
        lookup_key.trim(),
        target_host.trim(),
        comment
    )?;
    Ok(())
}

pub async fn remove_endpoint_override(pool: &DbPool, lookup_key: &str) -> Result<bool> {
    let affected = match pool {
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM dns_overrides WHERE lookup_key = ?")
            .bind(lookup_key)
            .execute(p)
            .await?
            .rows_affected(),
        DbPool::Postgres(p) => {
            sqlx::query(&pg_sql("DELETE FROM dns_overrides WHERE lookup_key = ?"))
                .bind(lookup_key)
                .execute(p)
                .await?
                .rows_affected()
        }
    };
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_memory_db;

    #[tokio::test]
    async fn set_get_list_endpoint_override() {
        let pool = init_memory_db().await.unwrap();
        set_endpoint_override(&pool, "mx.example.org", "relay.other.org", "test")
            .await
            .unwrap();

        let row = get_endpoint_override(&pool, "mx.example.org")
            .await
            .unwrap()
            .expect("override exists");
        assert_eq!(row.lookup_key, "mx.example.org");
        assert_eq!(row.target_host, "relay.other.org");
        assert_eq!(row.comment.as_deref(), Some("test"));

        let all = list_endpoint_overrides(&pool).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].lookup_key, "mx.example.org");
    }

    #[tokio::test]
    async fn set_endpoint_override_upserts_and_trims() {
        let pool = init_memory_db().await.unwrap();
        set_endpoint_override(&pool, "  key  ", "  host  ", "first")
            .await
            .unwrap();
        set_endpoint_override(&pool, "key", "new-host", "updated")
            .await
            .unwrap();

        let row = get_endpoint_override(&pool, "key").await.unwrap().unwrap();
        assert_eq!(row.lookup_key, "key");
        assert_eq!(row.target_host, "new-host");
        assert_eq!(row.comment.as_deref(), Some("updated"));

        let all = list_endpoint_overrides(&pool).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn set_endpoint_override_rejects_empty_fields() {
        let pool = init_memory_db().await.unwrap();
        let err = set_endpoint_override(&pool, " ", "host", "")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("LOOKUP_KEY"));
        let err = set_endpoint_override(&pool, "key", "", "")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("TARGET_HOST"));
    }

    #[tokio::test]
    async fn remove_endpoint_override_deletes_row() {
        let pool = init_memory_db().await.unwrap();
        set_endpoint_override(&pool, "gone", "host", "")
            .await
            .unwrap();
        assert!(super::remove_endpoint_override(&pool, "gone")
            .await
            .unwrap());
        assert!(!super::remove_endpoint_override(&pool, "gone")
            .await
            .unwrap());
        assert!(get_endpoint_override(&pool, "gone")
            .await
            .unwrap()
            .is_none());
    }
}
