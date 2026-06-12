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

//! Unified SQLx pool (SQLite or PostgreSQL).

use chatmail_config::{DatabaseConfig, DbDriver};
use chatmail_types::{ChatmailError, Result};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

/// Application database pool (`auth.pass_table` / credentials DB).
#[derive(Clone)]
pub enum DbPool {
    Sqlite(SqlitePool),
    Postgres(sqlx::PgPool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbBackend {
    Sqlite,
    Postgres,
}

impl DbPool {
    pub fn backend(&self) -> DbBackend {
        match self {
            Self::Sqlite(_) => DbBackend::Sqlite,
            Self::Postgres(_) => DbBackend::Postgres,
        }
    }

    pub fn is_postgres(&self) -> bool {
        matches!(self.backend(), DbBackend::Postgres)
    }
}

/// Fetch zero or one row (`?` placeholders; adapted for PostgreSQL).
#[macro_export]
macro_rules! db_fetch_optional {
    ($pool:expr, $ty:ty, $sql:expr $(, $bind:expr)*) => {{
        match &$pool {
            $crate::DbPool::Sqlite(__p) => {
                sqlx::query_as::<_, $ty>($sql)
                    $(.bind($bind))*
                    .fetch_optional(__p)
                    .await
                    .map_err(chatmail_types::ChatmailError::from)
            }
            $crate::DbPool::Postgres(__p) => {
                let __pg_sql = $crate::pool::pg_sql($sql);
                sqlx::query_as::<_, $ty>(&__pg_sql)
                    $(.bind($bind))*
                    .fetch_optional(__p)
                    .await
                    .map_err(chatmail_types::ChatmailError::from)
            }
        }
    }};
}

/// Fetch exactly one row.
#[macro_export]
macro_rules! db_fetch_one {
    ($pool:expr, $ty:ty, $sql:expr $(, $bind:expr)*) => {{
        match &$pool {
            $crate::DbPool::Sqlite(__p) => {
                sqlx::query_as::<_, $ty>($sql)
                    $(.bind($bind))*
                    .fetch_one(__p)
                    .await
                    .map_err(chatmail_types::ChatmailError::from)
            }
            $crate::DbPool::Postgres(__p) => {
                let __pg_sql = $crate::pool::pg_sql($sql);
                sqlx::query_as::<_, $ty>(&__pg_sql)
                    $(.bind($bind))*
                    .fetch_one(__p)
                    .await
                    .map_err(chatmail_types::ChatmailError::from)
            }
        }
    }};
}

/// Fetch all matching rows.
#[macro_export]
macro_rules! db_fetch_all {
    ($pool:expr, $ty:ty, $sql:expr $(, $bind:expr)*) => {{
        match &$pool {
            $crate::DbPool::Sqlite(__p) => {
                sqlx::query_as::<_, $ty>($sql)
                    $(.bind($bind))*
                    .fetch_all(__p)
                    .await
                    .map_err(chatmail_types::ChatmailError::from)
            }
            $crate::DbPool::Postgres(__p) => {
                let __pg_sql = $crate::pool::pg_sql($sql);
                sqlx::query_as::<_, $ty>(&__pg_sql)
                    $(.bind($bind))*
                    .fetch_all(__p)
                    .await
                    .map_err(chatmail_types::ChatmailError::from)
            }
        }
    }};
}

/// Fetch a single scalar column (first column of one row).
#[macro_export]
macro_rules! db_fetch_scalar {
    ($pool:expr, $ty:ty, $sql:expr $(, $bind:expr)*) => {{
        $crate::db_fetch_one!($pool, ($ty,), $sql $(, $bind)*).map(|(v,)| v)
    }};
}

/// Run a statement without a result set.
#[macro_export]
macro_rules! db_execute {
    ($pool:expr, $sql:expr $(, $bind:expr)*) => {{
        match &$pool {
            $crate::DbPool::Sqlite(__p) => {
                sqlx::query($sql)
                    $(.bind($bind))*
                    .execute(__p)
                    .await
                    .map(|_| ())
            }
            $crate::DbPool::Postgres(__p) => {
                let __pg_sql = $crate::pool::pg_sql($sql);
                sqlx::query(&__pg_sql)
                    $(.bind($bind))*
                    .execute(__p)
                    .await
                    .map(|_| ())
            }
        }
        .map_err(chatmail_types::ChatmailError::from)
    }};
}

pub async fn connect_database(config: &DatabaseConfig) -> Result<DbPool> {
    match config.driver {
        DbDriver::Sqlite3 => connect_sqlite(Path::new(&config.dsn)).await,
        DbDriver::Postgres => connect_postgres(&config.dsn).await,
    }
}

async fn connect_sqlite(db_path: &Path) -> Result<DbPool> {
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

    let pool = SqlitePoolOptions::new()
        .max_connections(64)
        .connect_with(options)
        .await?;

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA busy_timeout = 30000")
        .execute(&pool)
        .await?;

    Ok(DbPool::Sqlite(pool))
}

async fn connect_postgres(dsn: &str) -> Result<DbPool> {
    let options = postgres_connect_options(dsn)?;
    let pool = PgPoolOptions::new()
        .max_connections(32)
        .connect_with(options)
        .await
        .map_err(ChatmailError::from)?;
    Ok(DbPool::Postgres(pool))
}

/// Madmail uses libpq `key=value` DSNs; sqlx expects a `postgres://` URL or [`PgConnectOptions`].
fn postgres_connect_options(dsn: &str) -> Result<PgConnectOptions> {
    let dsn = dsn.trim();
    if dsn.starts_with("postgres://") || dsn.starts_with("postgresql://") {
        return PgConnectOptions::from_str(dsn).map_err(ChatmailError::from);
    }
    let params = parse_libpq_dsn(dsn).map_err(ChatmailError::config)?;
    let mut opts = PgConnectOptions::new_without_pgpass();
    if let Some(host) = params.get("host") {
        opts = opts.host(host);
    }
    if let Some(port) = params.get("port") {
        opts = opts.port(
            port.parse()
                .map_err(|_| ChatmailError::config(format!("invalid postgres port: {port}")))?,
        );
    }
    if let Some(user) = params.get("user") {
        opts = opts.username(user);
    }
    if let Some(password) = params.get("password") {
        opts = opts.password(password);
    }
    if let Some(dbname) = params.get("dbname") {
        opts = opts.database(dbname);
    }
    if let Some(sslmode) = params.get("sslmode") {
        opts = opts.ssl_mode(
            sslmode
                .parse()
                .map_err(|_| ChatmailError::config(format!("invalid sslmode: {sslmode}")))?,
        );
    }
    Ok(opts)
}

/// Parse a libpq connection string (`host=… user=… password=… dbname=…`).
fn parse_libpq_dsn(dsn: &str) -> std::result::Result<HashMap<String, String>, String> {
    let mut params = HashMap::new();
    let mut key = String::new();
    let mut value = String::new();
    let mut in_key = true;
    let mut in_quotes = false;
    let mut chars = dsn.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_key {
            if ch == '=' {
                in_key = false;
            } else if !ch.is_whitespace() {
                key.push(ch);
            }
        } else if in_quotes {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    value.push(next);
                }
            } else if ch == '"' {
                in_quotes = false;
            } else {
                value.push(ch);
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch.is_whitespace() {
            if !key.is_empty() {
                params.insert(key.clone(), value.clone());
                key.clear();
                value.clear();
                in_key = true;
            }
        } else {
            value.push(ch);
        }
    }
    if !key.is_empty() {
        params.insert(key, value);
    }
    if params.is_empty() {
        return Err("empty postgres DSN".into());
    }
    Ok(params)
}

pub(crate) async fn run_migrations(pool: &DbPool) -> Result<()> {
    match pool {
        DbPool::Sqlite(p) => sqlx::migrate!("./migrations/sqlite")
            .run(p)
            .await
            .map_err(map_migration_error)?,
        DbPool::Postgres(p) => {
            if crate::schema::madmail_postgres_schema_present(pool).await? {
                tracing::info!(
                    "skipping madmail-v2 PostgreSQL migrations (existing Madmail schema)"
                );
                apply_postgres_extension_tables(p).await?;
            } else {
                sqlx::migrate!("./migrations/postgres")
                    .run(p)
                    .await
                    .map_err(map_migration_error)?;
            }
        }
    }
    Ok(())
}

/// Tables added by madmail-v2 that are not created by Madmail Go migrations.
async fn apply_postgres_extension_tables(pool: &sqlx::PgPool) -> Result<()> {
    const FEDERATION_SILENT_DISMISS: &str =
        include_str!("../migrations/postgres/20240501000000_federation_silent_dismiss.sql");
    sqlx::query(FEDERATION_SILENT_DISMISS)
        .execute(pool)
        .await
        .map_err(ChatmailError::from)?;
    const MAILBOX_MODSEQ: &str =
        include_str!("../migrations/postgres/20240601000000_mailbox_modseq.sql");
    sqlx::query(MAILBOX_MODSEQ)
        .execute(pool)
        .await
        .map_err(ChatmailError::from)?;
    Ok(())
}

/// Rewrite SQLite `?` placeholders to PostgreSQL `$1`, `$2`, …
pub fn pg_sql(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut index = 1usize;
    for ch in sql.chars() {
        if ch == '?' {
            out.push('$');
            out.push_str(&index.to_string());
            index += 1;
        } else {
            out.push(ch);
        }
    }
    out
}

fn map_migration_error(e: sqlx::migrate::MigrateError) -> ChatmailError {
    if let sqlx::migrate::MigrateError::VersionMismatch(version) = e {
        return ChatmailError::config(format!(
            "migration {version} no longer matches the database checksum (the .sql file changed after it was applied). \
             For local SQLite dev, run `make reset-db` then `make restart`"
        ));
    }
    ChatmailError::Db(sqlx::Error::Migrate(Box::new(e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_libpq_dsn_fields() {
        let dsn =
            "host=127.0.0.1 port=5432 user=maddy password=secret dbname=maddy sslmode=disable";
        let p = parse_libpq_dsn(dsn).unwrap();
        assert_eq!(p.get("host").map(String::as_str), Some("127.0.0.1"));
        assert_eq!(p.get("port").map(String::as_str), Some("5432"));
        assert_eq!(p.get("user").map(String::as_str), Some("maddy"));
        assert_eq!(p.get("password").map(String::as_str), Some("secret"));
        assert_eq!(p.get("dbname").map(String::as_str), Some("maddy"));
    }

    #[test]
    fn postgres_connect_options_from_libpq() {
        postgres_connect_options(
            "host=127.0.0.1 port=5432 user=test password=test dbname=test sslmode=disable",
        )
        .expect("libpq DSN should parse");
    }
}
