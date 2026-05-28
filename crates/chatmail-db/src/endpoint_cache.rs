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

#[derive(Debug, Clone)]
pub struct EndpointOverrideRow {
    pub lookup_key: String,
    pub target_host: String,
    pub comment: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

const SELECT_ALL: &str = "SELECT lookup_key, target_host, comment, created_at, updated_at
         FROM dns_overrides ORDER BY created_at DESC";

const SELECT_ONE: &str = "SELECT lookup_key, target_host, comment, created_at, updated_at
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
    let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>)> =
        db_fetch_all!(
            pool,
            (String, String, Option<String>, Option<String>, Option<String>),
            SELECT_ALL
        )?;
    Ok(rows.into_iter().map(map_row).collect())
}

pub async fn get_endpoint_override(
    pool: &DbPool,
    lookup_key: &str,
) -> Result<Option<EndpointOverrideRow>> {
    let row: Option<(String, String, Option<String>, Option<String>, Option<String>)> =
        db_fetch_optional!(
            pool,
            (String, String, Option<String>, Option<String>, Option<String>),
            SELECT_ONE,
            lookup_key
        )?;
    Ok(row.map(map_row))
}

pub async fn set_endpoint_override(
    pool: &DbPool,
    lookup_key: &str,
    target_host: &str,
    comment: &str,
) -> Result<()> {
    if lookup_key.trim().is_empty() || target_host.trim().is_empty() {
        return Err(ChatmailError::config("LOOKUP_KEY and TARGET_HOST are required"));
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
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM dns_overrides WHERE lookup_key = ?")
                .bind(lookup_key)
                .execute(p)
                .await?
                .rows_affected()
        }
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
