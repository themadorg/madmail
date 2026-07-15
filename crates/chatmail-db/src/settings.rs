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

use std::collections::HashMap;

use chatmail_types::Result;

use crate::pool::DbBackend;
use crate::schema::{has_settings_table, uses_madmail_settings_kv};
use crate::{db_execute, db_fetch_all, db_fetch_optional, DbPool};

fn runtime_keys_sql(table: &str, backend: DbBackend) -> String {
    match backend {
        DbBackend::Sqlite => format!("SELECT key, value FROM {table} WHERE key GLOB '__*__'"),
        DbBackend::Postgres => {
            format!("SELECT key, value FROM {table} WHERE key ~ '^__.*__$'")
        }
    }
}

async fn fetch_settings_in_keys(
    pool: &DbPool,
    table: &str,
    keys: &[&str],
) -> Result<Vec<(String, String)>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let prefix = format!("SELECT key, value FROM {table} WHERE key IN (");
    match pool {
        DbPool::Sqlite(p) => {
            let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(prefix);
            let mut sep = qb.separated(", ");
            for key in keys {
                sep.push_bind(key);
            }
            qb.push(")");
            Ok(qb.build_query_as().fetch_all(p).await?)
        }
        DbPool::Postgres(p) => {
            let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(prefix);
            let mut sep = qb.separated(", ");
            for key in keys {
                sep.push_bind(key);
            }
            qb.push(")");
            Ok(qb.build_query_as().fetch_all(p).await?)
        }
    }
}

async fn get_setting_madmail_kv(pool: &DbPool, key: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = db_fetch_optional!(
        pool,
        (String,),
        "SELECT value FROM passwords WHERE key = ?",
        key
    )?;
    Ok(row.map(|(value,)| value))
}

async fn set_setting_madmail_kv(pool: &DbPool, key: &str, value: &str) -> Result<()> {
    db_execute!(
        pool,
        "INSERT INTO passwords (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        key,
        value
    )?;
    Ok(())
}

async fn delete_setting_madmail_kv(pool: &DbPool, key: &str) -> Result<()> {
    db_execute!(pool, "DELETE FROM passwords WHERE key = ?", key)?;
    Ok(())
}

fn is_madmail_runtime_key(key: &str) -> bool {
    key.starts_with("__") && key.ends_with("__")
}

pub async fn get_setting(pool: &DbPool, key: &str) -> Result<Option<String>> {
    if uses_madmail_settings_kv(pool).await? && is_madmail_runtime_key(key) {
        if let Some(v) = get_setting_madmail_kv(pool, key).await? {
            return Ok(Some(v));
        }
    }
    if has_settings_table(pool).await? {
        let row: Option<(String,)> = db_fetch_optional!(
            pool,
            (String,),
            "SELECT value FROM settings WHERE key = ?",
            key
        )?;
        if row.is_some() {
            return Ok(row.map(|(value,)| value));
        }
    }
    if uses_madmail_settings_kv(pool).await? {
        return get_setting_madmail_kv(pool, key).await;
    }
    Ok(None)
}

pub async fn set_setting(pool: &DbPool, key: &str, value: &str) -> Result<()> {
    if uses_madmail_settings_kv(pool).await? && is_madmail_runtime_key(key) {
        set_setting_madmail_kv(pool, key, value).await?;
        if has_settings_table(pool).await? {
            db_execute!(
                pool,
                "INSERT INTO settings (key, value) VALUES (?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                key,
                value
            )?;
        }
        return Ok(());
    }
    if has_settings_table(pool).await? {
        db_execute!(
            pool,
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            key,
            value
        )?;
        return Ok(());
    }
    if uses_madmail_settings_kv(pool).await? {
        return set_setting_madmail_kv(pool, key, value).await;
    }
    Err(chatmail_types::ChatmailError::config(
        "no settings table and passwords is not Madmail key/value layout",
    ))
}

pub async fn get_settings_many(pool: &DbPool, keys: &[&str]) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    if keys.is_empty() {
        return Ok(out);
    }

    let use_settings = has_settings_table(pool).await?;
    let use_kv = uses_madmail_settings_kv(pool).await?;

    if use_settings {
        for (k, v) in fetch_settings_in_keys(pool, "settings", keys).await? {
            out.insert(k, v);
        }
    }

    if use_kv {
        for (k, v) in fetch_settings_in_keys(pool, "passwords", keys).await? {
            out.entry(k).or_insert(v);
        }
    }

    Ok(out)
}

pub async fn list_double_underscore_settings(pool: &DbPool) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    let backend = pool.backend();
    if has_settings_table(pool).await? {
        let sql = runtime_keys_sql("settings", backend);
        let rows: Vec<(String, String)> = db_fetch_all!(pool, (String, String), sql.as_str())?;
        out.extend(rows);
    }
    if uses_madmail_settings_kv(pool).await? {
        let sql = runtime_keys_sql("passwords", backend);
        let rows: Vec<(String, String)> = db_fetch_all!(pool, (String, String), sql.as_str())?;
        for row in rows {
            if !out.iter().any(|(k, _)| k == &row.0) {
                out.push(row);
            }
        }
    }
    Ok(out)
}

pub async fn delete_setting(pool: &DbPool, key: &str) -> Result<()> {
    if has_settings_table(pool).await? {
        db_execute!(pool, "DELETE FROM settings WHERE key = ?", key)?;
    }
    if uses_madmail_settings_kv(pool).await? {
        delete_setting_madmail_kv(pool, key).await?;
    }
    Ok(())
}

pub async fn seed_install_defaults(pool: &DbPool) -> Result<()> {
    use crate::settings_keys;
    use chatmail_config::{DEFAULT_MAX_FEDERATION_SIZE, DEFAULT_MAX_MESSAGE_SIZE};
    seed_bool_if_unset(pool, settings_keys::JIT_REGISTRATION_ENABLED, true).await?;
    seed_bool_if_unset(pool, settings_keys::REGISTRATION_OPEN, true).await?;
    seed_string_if_unset(pool, settings_keys::APPENDLIMIT, DEFAULT_MAX_MESSAGE_SIZE).await?;
    seed_string_if_unset(
        pool,
        settings_keys::MAX_MESSAGE_SIZE,
        DEFAULT_MAX_MESSAGE_SIZE,
    )
    .await?;
    seed_string_if_unset(
        pool,
        settings_keys::MAX_FEDERATION_SIZE,
        DEFAULT_MAX_FEDERATION_SIZE,
    )
    .await?;
    seed_string_if_unset(pool, settings_keys::PUSH_MODE, "off").await?;
    seed_bool_if_unset(pool, settings_keys::PUSH_ENABLED, false).await?;
    Ok(())
}

async fn seed_string_if_unset(pool: &DbPool, key: &str, value: &str) -> Result<()> {
    if get_setting(pool, key).await?.is_none() {
        set_setting(pool, key, value).await?;
    }
    Ok(())
}

async fn seed_bool_if_unset(pool: &DbPool, key: &str, value: bool) -> Result<()> {
    if get_setting(pool, key).await?.is_none() {
        set_setting(pool, key, if value { "true" } else { "false" }).await?;
    }
    Ok(())
}

pub async fn get_bool_setting(pool: &DbPool, key: &str, default: bool) -> Result<bool> {
    match get_setting(pool, key).await? {
        None => Ok(default),
        // true/yes/1/enable/… ; false/no/0/disable/… ; unknown → false
        Some(value) => Ok(chatmail_config::parse_bool_str(&value)),
    }
}

pub async fn get_enabled_setting(pool: &DbPool, key: &str, default_enabled: bool) -> Result<bool> {
    match get_setting(pool, key).await? {
        None => Ok(default_enabled),
        // Same flexible forms as get_bool_setting (`enabled`, `true`, `yes`, `1`, …)
        Some(value) => Ok(chatmail_config::parse_bool_str(&value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_memory_db;
    use crate::settings_keys::{self, JIT_REGISTRATION_ENABLED, REGISTRATION_OPEN};

    #[tokio::test]
    async fn get_settings_many_batch_load() {
        let pool = init_memory_db().await.unwrap();
        set_setting(&pool, settings_keys::SMTP_PORT, "2525")
            .await
            .unwrap();
        set_setting(&pool, settings_keys::LANGUAGE, "fa")
            .await
            .unwrap();
        let map = get_settings_many(
            &pool,
            &[
                settings_keys::SMTP_PORT,
                settings_keys::LANGUAGE,
                settings_keys::IMAP_PORT,
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            map.get(settings_keys::SMTP_PORT).map(String::as_str),
            Some("2525")
        );
        assert_eq!(
            map.get(settings_keys::LANGUAGE).map(String::as_str),
            Some("fa")
        );
        assert!(!map.contains_key(settings_keys::IMAP_PORT));
    }

    #[tokio::test]
    async fn p1_ut04_settings_crud() {
        let pool = init_memory_db().await.unwrap();
        set_setting(&pool, "PORT", "25").await.unwrap();
        assert_eq!(
            get_setting(&pool, "PORT").await.unwrap().as_deref(),
            Some("25")
        );
        set_setting(&pool, "PORT", "587").await.unwrap();
        assert_eq!(
            get_setting(&pool, "PORT").await.unwrap().as_deref(),
            Some("587")
        );
        delete_setting(&pool, "PORT").await.unwrap();
        assert!(get_setting(&pool, "PORT").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn seed_install_defaults_sets_message_size_and_registration() {
        let pool = init_memory_db().await.unwrap();
        seed_install_defaults(&pool).await.unwrap();
        assert_eq!(
            get_setting(&pool, settings_keys::APPENDLIMIT)
                .await
                .unwrap()
                .as_deref(),
            Some("100M")
        );
        assert_eq!(
            get_setting(&pool, settings_keys::MAX_MESSAGE_SIZE)
                .await
                .unwrap()
                .as_deref(),
            Some("100M")
        );
        assert!(get_bool_setting(&pool, JIT_REGISTRATION_ENABLED, false)
            .await
            .unwrap());
        assert!(get_bool_setting(&pool, REGISTRATION_OPEN, false)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn p1_ut05_bool_setting_defaults() {
        let pool = init_memory_db().await.unwrap();
        assert!(!get_bool_setting(&pool, "MISSING_KEY", false).await.unwrap());
        assert!(
            !get_bool_setting(&pool, settings_keys::REGISTRATION_OPEN, false)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn p1_ut05_bool_setting_true_and_false() {
        let pool = init_memory_db().await.unwrap();
        set_setting(&pool, settings_keys::REGISTRATION_OPEN, "true")
            .await
            .unwrap();
        assert!(
            get_bool_setting(&pool, settings_keys::REGISTRATION_OPEN, false)
                .await
                .unwrap()
        );
        set_setting(&pool, settings_keys::REGISTRATION_OPEN, "false")
            .await
            .unwrap();
        assert!(
            !get_bool_setting(&pool, settings_keys::REGISTRATION_OPEN, true)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn bool_setting_accepts_flexible_enable_disable_forms() {
        let pool = init_memory_db().await.unwrap();
        for on in ["True", "yes", "1", "enable", "enabled", "on", "Y"] {
            set_setting(&pool, settings_keys::REGISTRATION_OPEN, on)
                .await
                .unwrap();
            assert!(
                get_bool_setting(&pool, settings_keys::REGISTRATION_OPEN, false)
                    .await
                    .unwrap(),
                "expected enabled for {on:?}"
            );
            set_setting(&pool, settings_keys::AUTO_PURGE_SEEN, on)
                .await
                .unwrap();
            assert!(
                get_enabled_setting(&pool, settings_keys::AUTO_PURGE_SEEN, false)
                    .await
                    .unwrap(),
                "expected get_enabled_setting for {on:?}"
            );
        }
        for off in ["False", "no", "0", "disable", "disabled", "off", "N"] {
            set_setting(&pool, settings_keys::REGISTRATION_OPEN, off)
                .await
                .unwrap();
            assert!(
                !get_bool_setting(&pool, settings_keys::REGISTRATION_OPEN, true)
                    .await
                    .unwrap(),
                "expected disabled for {off:?}"
            );
            set_setting(&pool, settings_keys::AUTO_PURGE_SEEN, off)
                .await
                .unwrap();
            assert!(
                !get_enabled_setting(&pool, settings_keys::AUTO_PURGE_SEEN, true)
                    .await
                    .unwrap(),
                "expected get_enabled_setting off for {off:?}"
            );
        }
    }

    #[tokio::test]
    async fn p1_ut06_sql_injection_protection() {
        let pool = init_memory_db().await.unwrap();
        let key = "key'\";\nDROP TABLE settings;--";
        let value = "value'\";\n'; DELETE FROM settings;--";
        set_setting(&pool, key, value).await.unwrap();
        assert_eq!(
            get_setting(&pool, key).await.unwrap().as_deref(),
            Some(value)
        );

        let DbPool::Sqlite(p) = &pool else {
            panic!("memory db is sqlite");
        };
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM settings")
            .fetch_one(p)
            .await
            .unwrap();
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn madmail_kv_settings_in_passwords_table() {
        let pool = init_memory_db().await.unwrap();
        let DbPool::Sqlite(p) = &pool else {
            panic!("memory db is sqlite");
        };
        sqlx::query("DROP TABLE settings").execute(p).await.unwrap();
        sqlx::query("DROP TABLE passwords")
            .execute(p)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE passwords (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(p)
            .await
            .unwrap();
        sqlx::query("INSERT INTO passwords (key, value) VALUES ('__IMAP_PORT__', '143')")
            .execute(p)
            .await
            .unwrap();

        assert_eq!(
            get_setting(&pool, "__IMAP_PORT__")
                .await
                .unwrap()
                .as_deref(),
            Some("143")
        );
        set_setting(&pool, "__IMAP_PORT__", "1143").await.unwrap();
        assert_eq!(
            get_setting(&pool, "__IMAP_PORT__")
                .await
                .unwrap()
                .as_deref(),
            Some("1143")
        );
    }
}
