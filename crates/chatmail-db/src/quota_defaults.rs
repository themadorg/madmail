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

//! Server-wide default storage quota (Madmail `GetDefaultQuota`).

use chatmail_config::{effective_default_quota_bytes as config_default_quota_bytes, AppConfig};
use chatmail_types::Result;

use crate::settings_keys::GLOBAL_QUOTA_USERNAME;
use crate::{db_fetch_optional, DbPool};

pub async fn resolve_default_quota_bytes(pool: &DbPool, config: &AppConfig) -> Result<u64> {
    let config_default = config_default_quota_bytes(config);
    let qt = crate::schema::quota_table(pool).await?;
    let sql = format!("SELECT max_storage FROM {qt} WHERE username = ?");
    let row: Option<(i64,)> = db_fetch_optional!(pool, (i64,), &sql, GLOBAL_QUOTA_USERNAME)?;
    Ok(match row {
        Some((m,)) if m > 0 => m as u64,
        _ => config_default,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{init_memory_db, settings_keys::GLOBAL_QUOTA_USERNAME};

    #[tokio::test]
    async fn global_default_row_overrides_config() {
        let pool = init_memory_db().await.unwrap();
        let mut cfg = chatmail_config::AppConfig::default();
        cfg.default_quota = Some("1G".into());
        let DbPool::Sqlite(p) = &pool else {
            panic!("memory db is sqlite");
        };
        sqlx::query(
            "INSERT INTO quotas (username, max_storage, created_at, first_login_at, last_login_at)
             VALUES (?, ?, 0, 0, 0)",
        )
        .bind(GLOBAL_QUOTA_USERNAME)
        .bind(2_i64 * 1024 * 1024 * 1024)
        .execute(p)
        .await
        .unwrap();
        let bytes = resolve_default_quota_bytes(&pool, &cfg).await.unwrap();
        assert_eq!(bytes, 2 * 1024 * 1024 * 1024);
    }
}
