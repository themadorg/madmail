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

//! Detect Madmail vs madmail-v2 database layouts.

use chatmail_types::Result;

use crate::pool::DbBackend;
use crate::{db_fetch_all, db_fetch_optional, DbPool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordsLayout {
    ChatmailRs,
    MadmailKv,
    Unknown,
}

pub async fn table_exists(pool: &DbPool, name: &str) -> Result<bool> {
    match pool.backend() {
        DbBackend::Sqlite => {
            let row: Option<(i32,)> = db_fetch_optional!(
                pool,
                (i32,),
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ? LIMIT 1",
                name
            )?;
            Ok(row.is_some())
        }
        DbBackend::Postgres => {
            let row: Option<(i32,)> = db_fetch_optional!(
                pool,
                (i32,),
                "SELECT 1 FROM information_schema.tables \
                 WHERE table_schema = 'public' AND table_name = ? LIMIT 1",
                name
            )?;
            Ok(row.is_some())
        }
    }
}

pub async fn passwords_layout(pool: &DbPool) -> Result<PasswordsLayout> {
    let cols = password_column_names(pool).await?;
    if cols.is_empty() {
        return Ok(PasswordsLayout::Unknown);
    }
    if cols.iter().any(|c| c == "username") {
        return Ok(PasswordsLayout::ChatmailRs);
    }
    if cols.iter().any(|c| c == "key") {
        return Ok(PasswordsLayout::MadmailKv);
    }
    Ok(PasswordsLayout::Unknown)
}

async fn password_column_names(pool: &DbPool) -> Result<Vec<String>> {
    match pool.backend() {
        DbBackend::Sqlite => {
            let cols: Vec<(String,)> = db_fetch_all!(
                pool,
                (String,),
                "SELECT name FROM pragma_table_info('passwords') ORDER BY cid"
            )?;
            Ok(cols.into_iter().map(|(n,)| n).collect())
        }
        DbBackend::Postgres => {
            let cols: Vec<(String,)> = db_fetch_all!(
                pool,
                (String,),
                "SELECT column_name FROM information_schema.columns \
                 WHERE table_schema = 'public' AND table_name = 'passwords' \
                 ORDER BY ordinal_position"
            )?;
            Ok(cols.into_iter().map(|(n,)| n).collect())
        }
    }
}

pub async fn has_settings_table(pool: &DbPool) -> Result<bool> {
    table_exists(pool, "settings").await
}

pub async fn uses_madmail_settings_kv(pool: &DbPool) -> Result<bool> {
    Ok(matches!(
        passwords_layout(pool).await?,
        PasswordsLayout::MadmailKv
    ))
}

/// Madmail Go uses `quota`; madmail-v2 migrations create `quotas`.
pub async fn quota_table(pool: &DbPool) -> Result<&'static str> {
    if table_exists(pool, "quota").await? {
        Ok("quota")
    } else {
        Ok("quotas")
    }
}

/// Existing Madmail PostgreSQL DB (from Go binary) — skip madmail-v2 migrations.
pub async fn madmail_postgres_schema_present(pool: &DbPool) -> Result<bool> {
    if !pool.is_postgres() {
        return Ok(false);
    }
    table_exists(pool, "schema_version").await
}

/// Madmail Go uses `failed_http_s` / `success_http_s`; madmail-v2 uses `failed_https` / `success_https`.
pub struct FederationStatsColumns {
    pub failed_https: &'static str,
    pub success_https: &'static str,
}

pub async fn federation_stats_columns(pool: &DbPool) -> Result<FederationStatsColumns> {
    let uses_http_s = match pool.backend() {
        DbBackend::Sqlite => {
            if !table_exists(pool, "federation_server_stats").await? {
                false
            } else {
                let cols: Vec<(String,)> = db_fetch_all!(
                    pool,
                    (String,),
                    "SELECT name FROM pragma_table_info('federation_server_stats')"
                )?;
                cols.iter().any(|(n,)| n == "failed_http_s")
            }
        }
        DbBackend::Postgres => db_fetch_optional!(
            pool,
            (i32,),
            "SELECT 1 FROM information_schema.columns \
                 WHERE table_schema = 'public' AND table_name = 'federation_server_stats' \
                 AND column_name = 'failed_http_s' LIMIT 1"
        )?
        .is_some(),
    };
    if uses_http_s {
        Ok(FederationStatsColumns {
            failed_https: "failed_http_s",
            success_https: "success_http_s",
        })
    } else {
        Ok(FederationStatsColumns {
            failed_https: "failed_https",
            success_https: "success_https",
        })
    }
}
