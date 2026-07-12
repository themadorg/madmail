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
    if crate::schema::legacy_madmail_schema_present(pool).await? {
        tracing::info!(
            backend = ?pool.backend(),
            "skipping madmail-v2 sqlx migrations (existing Madmail / go-imap-sql schema); ensuring application tables"
        );
        apply_legacy_schema_tables(pool).await?;
        return Ok(());
    }

    match pool {
        DbPool::Sqlite(p) => sqlx::migrate!("./migrations/sqlite")
            .run(p)
            .await
            .map_err(map_migration_error)?,
        DbPool::Postgres(p) => sqlx::migrate!("./migrations/postgres")
            .run(p)
            .await
            .map_err(map_migration_error)?,
    }
    Ok(())
}

/// Ensure application tables when skipping full sqlx migrations.
///
/// Go Madmail / go-imap-sql databases already have some GORM tables (and often
/// `schema_version`). Bare sqlx `CREATE TABLE` migrations then fail (#67:
/// `registration_tokens already exists`) or are skipped on Postgres when
/// `schema_version` is present — leaving gaps such as missing `federation_rules`
/// on pre-federation installs (v0.28 → 2.x).
///
/// Some Go / SQLite→Postgres imports leave tables **without** PRIMARY KEY or
/// UNIQUE constraints (seen on `message_stats`, `blocked_users`, `dns_overrides`).
/// `CREATE TABLE IF NOT EXISTS` is a no-op for those, but seed/runtime
/// `ON CONFLICT (col)` then fails with:
/// `there is no unique or exclusion constraint matching the ON CONFLICT specification`.
/// Unique indexes are therefore created with `IF NOT EXISTS` before any upsert.
///
/// Every statement uses `IF NOT EXISTS` / upsert no-ops so complete schemas are
/// left unchanged.
async fn apply_legacy_schema_tables(pool: &DbPool) -> Result<()> {
    let stmts = match pool.backend() {
        DbBackend::Sqlite => SQLITE_ENSURE_TABLE_STATEMENTS,
        DbBackend::Postgres => POSTGRES_ENSURE_TABLE_STATEMENTS,
    };
    for sql in stmts {
        match pool {
            DbPool::Sqlite(p) => {
                sqlx::query(sql)
                    .execute(p)
                    .await
                    .map_err(ChatmailError::from)?;
            }
            DbPool::Postgres(p) => {
                sqlx::query(sql)
                    .execute(p)
                    .await
                    .map_err(ChatmailError::from)?;
            }
        }
    }
    Ok(())
}

/// Single-statement DDL/DML for the SQLite legacy-schema ensure path.
const SQLITE_ENSURE_TABLE_STATEMENTS: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
)"#,
    r#"CREATE TABLE IF NOT EXISTS quotas (
    username TEXT PRIMARY KEY NOT NULL,
    max_storage INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT 0,
    first_login_at INTEGER NOT NULL DEFAULT 0,
    last_login_at INTEGER NOT NULL DEFAULT 0,
    used_token TEXT
)"#,
    r#"CREATE TABLE IF NOT EXISTS blocked_users (
    username TEXT PRIMARY KEY NOT NULL,
    reason TEXT NOT NULL,
    blocked_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)"#,
    // Existing GORM/import tables may lack a UNIQUE on username (ON CONFLICT needs it).
    r#"CREATE UNIQUE INDEX IF NOT EXISTS blocked_users_username_key ON blocked_users (username)"#,
    r#"CREATE TABLE IF NOT EXISTS registration_tokens (
    token TEXT PRIMARY KEY NOT NULL,
    max_uses INTEGER NOT NULL DEFAULT 1,
    used_count INTEGER NOT NULL DEFAULT 0,
    comment TEXT,
    expires_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS registration_tokens_token_key ON registration_tokens (token)"#,
    r#"CREATE TABLE IF NOT EXISTS dns_overrides (
    lookup_key TEXT PRIMARY KEY NOT NULL,
    target_host TEXT NOT NULL,
    comment TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS dns_overrides_lookup_key_key ON dns_overrides (lookup_key)"#,
    // Only when missing; existing Madmail `key`/`value` passwords tables are kept.
    r#"CREATE TABLE IF NOT EXISTS passwords (
    username TEXT PRIMARY KEY NOT NULL,
    hash TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
)"#,
    r#"CREATE TABLE IF NOT EXISTS push_tokens (
    username TEXT NOT NULL,
    device_token TEXT NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (username, device_token)
)"#,
    r#"CREATE TABLE IF NOT EXISTS federation_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    domain TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS federation_rules_domain_key ON federation_rules (domain)"#,
    r#"CREATE TABLE IF NOT EXISTS federation_server_stats (
    domain TEXT PRIMARY KEY NOT NULL,
    queued_messages INTEGER NOT NULL DEFAULT 0,
    failed_http INTEGER NOT NULL DEFAULT 0,
    failed_https INTEGER NOT NULL DEFAULT 0,
    failed_smtp INTEGER NOT NULL DEFAULT 0,
    success_http INTEGER NOT NULL DEFAULT 0,
    success_https INTEGER NOT NULL DEFAULT 0,
    success_smtp INTEGER NOT NULL DEFAULT 0,
    inbound_deliveries INTEGER NOT NULL DEFAULT 0,
    successful_deliveries INTEGER NOT NULL DEFAULT 0,
    total_latency_ms INTEGER NOT NULL DEFAULT 0,
    last_active INTEGER NOT NULL DEFAULT 0
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS federation_server_stats_domain_key ON federation_server_stats (domain)"#,
    r#"CREATE TABLE IF NOT EXISTS message_stats (
    name TEXT PRIMARY KEY NOT NULL,
    count INTEGER NOT NULL DEFAULT 0
)"#,
    // Critical: Go / import schemas often have message_stats with no UNIQUE on name.
    r#"CREATE UNIQUE INDEX IF NOT EXISTS message_stats_name_key ON message_stats (name)"#,
    r#"INSERT OR IGNORE INTO message_stats (name, count) VALUES
    ('sent_messages', 0),
    ('outbound_messages', 0),
    ('received_messages', 0)"#,
    r#"CREATE TABLE IF NOT EXISTS exchangers (
    name TEXT PRIMARY KEY NOT NULL,
    url TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    poll_interval INTEGER NOT NULL DEFAULT 60,
    last_poll_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS exchangers_name_key ON exchangers (name)"#,
    r#"CREATE TABLE IF NOT EXISTS federation_silent_dismiss (
    domain TEXT PRIMARY KEY NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS federation_silent_dismiss_domain_key ON federation_silent_dismiss (domain)"#,
    r#"CREATE TABLE IF NOT EXISTS mailbox_modseq (
    username TEXT PRIMARY KEY NOT NULL,
    modseq INTEGER NOT NULL DEFAULT 0
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS mailbox_modseq_username_key ON mailbox_modseq (username)"#,
];

/// Single-statement DDL/DML for the PostgreSQL legacy-schema ensure path.
const POSTGRES_ENSURE_TABLE_STATEMENTS: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
)"#,
    r#"CREATE TABLE IF NOT EXISTS quotas (
    username TEXT PRIMARY KEY NOT NULL,
    max_storage BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL DEFAULT 0,
    first_login_at BIGINT NOT NULL DEFAULT 0,
    last_login_at BIGINT NOT NULL DEFAULT 0,
    used_token TEXT
)"#,
    r#"CREATE TABLE IF NOT EXISTS blocked_users (
    username TEXT PRIMARY KEY NOT NULL,
    reason TEXT NOT NULL,
    blocked_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)"#,
    // Existing GORM/import tables may lack a UNIQUE on username (ON CONFLICT needs it).
    r#"CREATE UNIQUE INDEX IF NOT EXISTS blocked_users_username_key ON blocked_users (username)"#,
    r#"CREATE TABLE IF NOT EXISTS registration_tokens (
    token TEXT PRIMARY KEY NOT NULL,
    max_uses INTEGER NOT NULL DEFAULT 1,
    used_count INTEGER NOT NULL DEFAULT 0,
    comment TEXT,
    expires_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS registration_tokens_token_key ON registration_tokens (token)"#,
    r#"CREATE TABLE IF NOT EXISTS dns_overrides (
    lookup_key TEXT PRIMARY KEY NOT NULL,
    target_host TEXT NOT NULL,
    comment TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS dns_overrides_lookup_key_key ON dns_overrides (lookup_key)"#,
    r#"CREATE TABLE IF NOT EXISTS passwords (
    username TEXT PRIMARY KEY NOT NULL,
    hash TEXT NOT NULL,
    created_at BIGINT NOT NULL DEFAULT (FLOOR(EXTRACT(EPOCH FROM NOW())))
)"#,
    r#"CREATE TABLE IF NOT EXISTS push_tokens (
    username TEXT NOT NULL,
    device_token TEXT NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (username, device_token)
)"#,
    r#"CREATE TABLE IF NOT EXISTS federation_rules (
    id SERIAL PRIMARY KEY,
    domain TEXT NOT NULL UNIQUE,
    created_at BIGINT NOT NULL DEFAULT (FLOOR(EXTRACT(EPOCH FROM NOW())))
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS federation_rules_domain_key ON federation_rules (domain)"#,
    r#"CREATE TABLE IF NOT EXISTS federation_server_stats (
    domain TEXT PRIMARY KEY NOT NULL,
    queued_messages BIGINT NOT NULL DEFAULT 0,
    failed_http BIGINT NOT NULL DEFAULT 0,
    failed_https BIGINT NOT NULL DEFAULT 0,
    failed_smtp BIGINT NOT NULL DEFAULT 0,
    success_http BIGINT NOT NULL DEFAULT 0,
    success_https BIGINT NOT NULL DEFAULT 0,
    success_smtp BIGINT NOT NULL DEFAULT 0,
    inbound_deliveries BIGINT NOT NULL DEFAULT 0,
    successful_deliveries BIGINT NOT NULL DEFAULT 0,
    total_latency_ms BIGINT NOT NULL DEFAULT 0,
    last_active BIGINT NOT NULL DEFAULT 0
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS federation_server_stats_domain_key ON federation_server_stats (domain)"#,
    r#"CREATE TABLE IF NOT EXISTS message_stats (
    name TEXT PRIMARY KEY NOT NULL,
    count BIGINT NOT NULL DEFAULT 0
)"#,
    // Critical: Go / import schemas often have message_stats with no UNIQUE on name.
    // Without this, seed INSERT … ON CONFLICT (name) fails on Postgres.
    r#"CREATE UNIQUE INDEX IF NOT EXISTS message_stats_name_key ON message_stats (name)"#,
    r#"INSERT INTO message_stats (name, count) VALUES
    ('sent_messages', 0),
    ('outbound_messages', 0),
    ('received_messages', 0)
ON CONFLICT (name) DO NOTHING"#,
    r#"CREATE TABLE IF NOT EXISTS exchangers (
    name TEXT PRIMARY KEY NOT NULL,
    url TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    poll_interval INTEGER NOT NULL DEFAULT 60,
    last_poll_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS exchangers_name_key ON exchangers (name)"#,
    r#"CREATE TABLE IF NOT EXISTS federation_silent_dismiss (
    domain TEXT PRIMARY KEY NOT NULL,
    created_at BIGINT NOT NULL DEFAULT (FLOOR(EXTRACT(EPOCH FROM NOW())))
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS federation_silent_dismiss_domain_key ON federation_silent_dismiss (domain)"#,
    r#"CREATE TABLE IF NOT EXISTS mailbox_modseq (
    username TEXT PRIMARY KEY NOT NULL,
    modseq BIGINT NOT NULL DEFAULT 0
)"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS mailbox_modseq_username_key ON mailbox_modseq (username)"#,
];

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

    /// Existing-schema skip path must create federation_rules (0.28 → 2.x Postgres upgrades).
    #[test]
    fn ensure_statements_cover_boot_required_tables() {
        for (label, stmts) in [
            ("sqlite", SQLITE_ENSURE_TABLE_STATEMENTS),
            ("postgres", POSTGRES_ENSURE_TABLE_STATEMENTS),
        ] {
            let joined = stmts.join("\n");
            for table in [
                "federation_rules",
                "message_stats",
                "federation_server_stats",
                "federation_silent_dismiss",
                "mailbox_modseq",
                "settings",
                "passwords",
                "registration_tokens",
            ] {
                assert!(
                    joined.contains(table),
                    "{label} ensure list missing table {table}"
                );
            }
            let federation = stmts
                .iter()
                .find(|s| s.contains("federation_rules") && s.contains("CREATE TABLE"))
                .expect("federation_rules CREATE");
            assert!(
                federation.contains("domain TEXT NOT NULL UNIQUE"),
                "{label} federation_rules domain unique"
            );
            // Unique indexes before ON CONFLICT seed/upsert (constraint-less GORM imports).
            for idx in [
                "message_stats_name_key",
                "blocked_users_username_key",
                "dns_overrides_lookup_key_key",
            ] {
                assert!(
                    joined.contains(idx),
                    "{label} ensure list missing unique index {idx}"
                );
            }
            let msg_stats_idx = stmts
                .iter()
                .position(|s| s.contains("message_stats_name_key"))
                .expect("message_stats unique index");
            let msg_stats_seed = stmts
                .iter()
                .position(|s| s.contains("INSERT") && s.contains("message_stats"))
                .expect("message_stats seed");
            assert!(
                msg_stats_idx < msg_stats_seed,
                "{label} message_stats unique index must precede seed INSERT"
            );
            for sql in stmts {
                let trimmed = sql.trim().trim_end_matches(';');
                assert!(
                    !trimmed.contains(';'),
                    "{label} multi-statement ensure SQL is not supported: {sql}"
                );
            }
        }
    }

    /// GitHub #67: Go Madmail already created `registration_tokens`; bare sqlx migrate fails.
    #[tokio::test]
    async fn legacy_sqlite_skips_migrate_when_registration_tokens_exist() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("imapsql-like.db");
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))
            .unwrap()
            .create_if_missing(true);
        let sqlite = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        // Minimal GORM-like tables from Go Madmail (imapsql / combined DB).
        sqlx::query(
            "CREATE TABLE registration_tokens (
                token TEXT PRIMARY KEY NOT NULL,
                max_uses INTEGER NOT NULL DEFAULT 1,
                used_count INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&sqlite)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE passwords (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            )",
        )
        .execute(&sqlite)
        .await
        .unwrap();
        sqlx::query("INSERT INTO passwords (key, value) VALUES ('user@test.local', 'hash')")
            .execute(&sqlite)
            .await
            .unwrap();

        let pool = DbPool::Sqlite(sqlite);
        assert!(crate::schema::legacy_madmail_schema_present(&pool)
            .await
            .unwrap());
        run_migrations(&pool)
            .await
            .expect("legacy ensure must not fail like bare CREATE TABLE");
        assert!(crate::schema::table_exists(&pool, "federation_rules")
            .await
            .unwrap());
        assert!(crate::schema::table_exists(&pool, "settings")
            .await
            .unwrap());
        // Madmail KV passwords shape preserved (not replaced with username/hash).
        assert!(matches!(
            crate::schema::passwords_layout(&pool).await.unwrap(),
            crate::schema::PasswordsLayout::MadmailKv
        ));
        // sqlx must not have taken ownership (still legacy path on re-run).
        assert!(crate::schema::legacy_madmail_schema_present(&pool)
            .await
            .unwrap());
        run_migrations(&pool).await.expect("idempotent ensure");
    }

    /// Constraint-less legacy tables (Postgres 0.28 import / GORM) must still allow
    /// `ON CONFLICT` seed — reproduces:
    /// `there is no unique or exclusion constraint matching the ON CONFLICT specification`.
    #[tokio::test]
    async fn legacy_ensure_adds_unique_when_message_stats_lacks_pk() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("constraintless.db");
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))
            .unwrap()
            .create_if_missing(true);
        let sqlite = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();

        // go-imap-sql marker + tables shaped like the reporter's Postgres dump:
        // present, but without PRIMARY KEY / UNIQUE on natural keys.
        sqlx::query("CREATE TABLE schema_version (version INTEGER NOT NULL)")
            .execute(&sqlite)
            .await
            .unwrap();
        sqlx::query("INSERT INTO schema_version (version) VALUES (6)")
            .execute(&sqlite)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE message_stats (
                name TEXT NOT NULL,
                count INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&sqlite)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE blocked_users (
                username TEXT NOT NULL,
                reason TEXT NOT NULL,
                blocked_at TIMESTAMP
            )",
        )
        .execute(&sqlite)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE dns_overrides (
                lookup_key TEXT NOT NULL,
                target_host TEXT NOT NULL,
                comment TEXT,
                created_at TIMESTAMP,
                updated_at TIMESTAMP
            )",
        )
        .execute(&sqlite)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE passwords (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            )",
        )
        .execute(&sqlite)
        .await
        .unwrap();

        let pool = DbPool::Sqlite(sqlite);
        assert!(crate::schema::legacy_madmail_schema_present(&pool)
            .await
            .unwrap());
        run_migrations(&pool)
            .await
            .expect("ensure must not fail ON CONFLICT without unique");

        // Unique indexes installed for runtime upserts.
        let idx: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE '%message_stats%'",
        )
        .fetch_all(match &pool {
            DbPool::Sqlite(p) => p,
            _ => unreachable!(),
        })
        .await
        .unwrap();
        assert!(
            idx.iter().any(|(n,)| n.contains("message_stats")),
            "expected message_stats unique index, got {idx:?}"
        );

        // Seed rows present; second ensure is idempotent.
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM message_stats WHERE name = 'sent_messages'")
                .fetch_one(match &pool {
                    DbPool::Sqlite(p) => p,
                    _ => unreachable!(),
                })
                .await
                .unwrap();
        assert_eq!(count.0, 1);
        run_migrations(&pool).await.expect("idempotent ensure");

        // Runtime-style ON CONFLICT must succeed after ensure.
        sqlx::query(
            "INSERT INTO message_stats (name, count) VALUES ('sent_messages', 42)
             ON CONFLICT(name) DO UPDATE SET count = excluded.count",
        )
        .execute(match &pool {
            DbPool::Sqlite(p) => p,
            _ => unreachable!(),
        })
        .await
        .expect("ON CONFLICT(name) after ensure");
        sqlx::query(
            "INSERT INTO blocked_users (username, reason) VALUES ('u@test', 'x')
             ON CONFLICT(username) DO UPDATE SET reason = excluded.reason",
        )
        .execute(match &pool {
            DbPool::Sqlite(p) => p,
            _ => unreachable!(),
        })
        .await
        .expect("ON CONFLICT(username) on blocked_users after ensure");
    }


    /// Postgres e2e: constraint-less message_stats (reporter schema). Opt-in via MADMAIL_TEST_PG_DSN.
    #[tokio::test]
    async fn legacy_postgres_constraintless_message_stats_ensure() {
        let Some(dsn) = std::env::var("MADMAIL_TEST_PG_DSN").ok().filter(|s| !s.is_empty()) else {
            eprintln!("skip: set MADMAIL_TEST_PG_DSN for Postgres e2e");
            return;
        };
        let config = chatmail_config::DatabaseConfig {
            driver: chatmail_config::DbDriver::Postgres,
            dsn,
        };
        let pool = crate::connect_database(&config).await.expect("connect postgres");
        assert!(
            crate::schema::legacy_madmail_schema_present(&pool)
                .await
                .unwrap(),
            "expected legacy schema detection"
        );
        // This is the boot path that failed for the reporter.
        super::run_migrations(&pool)
            .await
            .expect("legacy ensure must succeed on constraint-less message_stats");
        super::run_migrations(&pool)
            .await
            .expect("idempotent ensure");

        // Runtime ON CONFLICT used by message_stats flush.
        match &pool {
            DbPool::Postgres(p) => {
                sqlx::query(
                    "INSERT INTO message_stats (name, count) VALUES ('sent_messages', 123)
                     ON CONFLICT (name) DO UPDATE SET count = excluded.count",
                )
                .execute(p)
                .await
                .expect("runtime ON CONFLICT after ensure");
                let row: (i64,) = sqlx::query_as(
                    "SELECT count FROM message_stats WHERE name = 'sent_messages'",
                )
                .fetch_one(p)
                .await
                .unwrap();
                assert_eq!(row.0, 123);
                // passwords KV preserved
                let layout = crate::schema::passwords_layout(&pool).await.unwrap();
                assert!(matches!(layout, crate::schema::PasswordsLayout::MadmailKv));
            }
            _ => panic!("expected postgres"),
        }
    }

    /// Fresh SQLite still gets full sqlx migrations (no legacy markers).
    #[tokio::test]
    async fn fresh_sqlite_still_runs_sqlx_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("fresh.db");
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))
            .unwrap()
            .create_if_missing(true);
        let sqlite = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let pool = DbPool::Sqlite(sqlite);
        assert!(!crate::schema::legacy_madmail_schema_present(&pool)
            .await
            .unwrap());
        run_migrations(&pool).await.unwrap();
        assert!(crate::schema::table_exists(&pool, "federation_rules")
            .await
            .unwrap());
        assert!(crate::schema::table_exists(&pool, "_sqlx_migrations")
            .await
            .unwrap());
        assert!(!crate::schema::legacy_madmail_schema_present(&pool)
            .await
            .unwrap());
    }
}
