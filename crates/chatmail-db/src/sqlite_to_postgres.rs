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

//! Copy application tables from a v2 SQLite file into PostgreSQL.
//!
//! Does not move Maildir, `sharing.db`, the retry queue, or `admin_token`.
//! The operator still points `driver` / `dsn` at Postgres and restarts.

use std::path::Path;

use chatmail_config::{DatabaseConfig, DbDriver};
use chatmail_types::{ChatmailError, Result};
use sqlx::postgres::PgPool;
use sqlx::{PgConnection, Row};

use crate::pool::{connect_database, run_migrations, DbPool};
use crate::schema::{
    federation_stats_columns, passwords_layout, quota_table, table_exists, PasswordsLayout,
};

/// Tables copied in this order (no cross-table FKs).
pub const COPY_TABLES: &[&str] = &[
    "settings",
    "quotas",
    "blocked_users",
    "registration_tokens",
    "dns_overrides",
    "passwords",
    "push_tokens",
    "federation_rules",
    "federation_server_stats",
    "message_stats",
    "exchangers",
    "federation_silent_dismiss",
    "mailbox_modseq",
];

#[derive(Debug, Clone, Default)]
pub struct CopyOpts {
    pub dry_run: bool,
    /// Replace existing Postgres rows. Without this, the copy is refused when any
    /// copied table except `message_stats` (sqlx seeds three zero counters) has rows.
    pub force: bool,
}

#[derive(Debug, Clone)]
pub struct TableCopy {
    pub table: String,
    /// SQLite table actually read (differs from `table` for legacy `quota`).
    pub source_table: String,
    pub sqlite_rows: u64,
    pub copied: u64,
    pub skipped: bool,
}

#[derive(Debug, Clone)]
pub struct CopyReport {
    pub sqlite_path: String,
    pub dry_run: bool,
    pub force: bool,
    pub tables: Vec<TableCopy>,
}

pub async fn inspect_sqlite_tables(sqlite_path: &Path) -> Result<Vec<TableCopy>> {
    let src = open_sqlite(sqlite_path).await?;
    inspect_tables(&src).await
}

async fn inspect_tables(src: &DbPool) -> Result<Vec<TableCopy>> {
    let mut tables = Vec::new();
    for name in COPY_TABLES {
        let source = source_table(src, name).await?;
        let exists = table_exists(src, &source).await?;
        let n = if exists {
            count_rows(src, &source).await?
        } else {
            0
        };
        tables.push(TableCopy {
            table: (*name).to_string(),
            source_table: source,
            sqlite_rows: n,
            copied: 0,
            skipped: !exists,
        });
    }
    Ok(tables)
}

/// SQLite table backing a destination table.
///
/// Go Madmail stores account records in singular `quota`; v2 uses `quotas` and
/// [`apply_legacy_schema_tables`](crate::pool) creates an *empty* `quotas` next to
/// it, so reading `quotas` blindly reports `0` rows and looks like success while
/// every account's quota and login timestamps are dropped.
async fn source_table(src: &DbPool, dest: &str) -> Result<String> {
    if dest == "quotas" {
        return Ok(quota_table(src).await?.to_string());
    }
    Ok(dest.to_string())
}

pub async fn copy_sqlite_to_postgres(
    sqlite_path: &Path,
    postgres_dsn: &str,
    opts: CopyOpts,
) -> Result<CopyReport> {
    let sqlite_path_s = sqlite_path.display().to_string();
    let src = open_sqlite(sqlite_path).await?;

    if opts.dry_run {
        return Ok(CopyReport {
            sqlite_path: sqlite_path_s,
            dry_run: true,
            force: opts.force,
            tables: inspect_tables(&src).await?,
        });
    }

    let dst_pool = connect_database(&DatabaseConfig {
        driver: DbDriver::Postgres,
        dsn: postgres_dsn.to_string(),
    })
    .await?;
    run_migrations(&dst_pool).await?;
    let pg = pg_pool(&dst_pool)?;

    // `run_migrations` deliberately skips the v2 schema on a Go-era database, so the
    // destination may still carry the Madmail key/value `passwords` table. Every INSERT
    // below writes `username`/`hash`, which would fail *after* --force had already
    // emptied the table. Refuse before touching anything.
    if passwords_layout(&dst_pool).await? == PasswordsLayout::MadmailKv {
        return Err(ChatmailError::config(
            "Postgres passwords table uses the legacy Madmail key/value layout. \
             Copy into a database with the madmail-v2 schema instead.",
        ));
    }

    // Everything from here runs in one transaction: on any error the destination is
    // left exactly as it was, including the --force deletes.
    let mut tx = pg.begin().await.map_err(ChatmailError::from)?;

    if !opts.force {
        let mut non_empty = Vec::new();
        for name in COPY_TABLES {
            // sqlx migrations INSERT three zero counters into message_stats, so a
            // brand-new Postgres database would always look occupied.
            if *name == "message_stats" {
                continue;
            }
            if count_pg(&mut tx, name).await? > 0 {
                non_empty.push(*name);
            }
        }
        if !non_empty.is_empty() {
            return Err(ChatmailError::config(format!(
                "Postgres already has rows in: {}. \
                 Refusing to overwrite (pass --force to replace).",
                non_empty.join(", ")
            )));
        }
    }

    if opts.force {
        for name in COPY_TABLES.iter().rev() {
            sqlx::query(&format!("DELETE FROM {name}"))
                .execute(&mut *tx)
                .await
                .map_err(ChatmailError::from)?;
        }
    }

    let mut tables = Vec::new();
    for name in COPY_TABLES {
        let source = source_table(&src, name).await?;
        if !table_exists(&src, &source).await? {
            tables.push(TableCopy {
                table: (*name).to_string(),
                source_table: source,
                sqlite_rows: 0,
                copied: 0,
                skipped: true,
            });
            continue;
        }
        let sqlite_rows = count_rows(&src, &source).await?;
        let copied = copy_one(&src, &mut tx, name, &source).await?;
        tables.push(TableCopy {
            table: (*name).to_string(),
            source_table: source,
            sqlite_rows,
            copied,
            skipped: false,
        });
    }

    tx.commit().await.map_err(ChatmailError::from)?;

    Ok(CopyReport {
        sqlite_path: sqlite_path_s,
        dry_run: false,
        force: opts.force,
        tables,
    })
}

async fn open_sqlite(path: &Path) -> Result<DbPool> {
    if !path.is_file() {
        return Err(ChatmailError::config(format!(
            "SQLite file not found: {}",
            path.display()
        )));
    }
    connect_database(&DatabaseConfig {
        driver: DbDriver::Sqlite3,
        dsn: path.display().to_string(),
    })
    .await
}

fn pg_pool(pool: &DbPool) -> Result<&PgPool> {
    match pool {
        DbPool::Postgres(p) => Ok(p),
        DbPool::Sqlite(_) => Err(ChatmailError::config(
            "internal error: expected Postgres pool",
        )),
    }
}

async fn count_rows(pool: &DbPool, table: &str) -> Result<u64> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let n: i64 = match pool {
        DbPool::Sqlite(p) => sqlx::query_scalar(&sql)
            .fetch_one(p)
            .await
            .map_err(ChatmailError::from)?,
        DbPool::Postgres(p) => sqlx::query_scalar(&sql)
            .fetch_one(p)
            .await
            .map_err(ChatmailError::from)?,
    };
    Ok(n.max(0) as u64)
}

async fn count_pg(conn: &mut PgConnection, table: &str) -> Result<u64> {
    let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&mut *conn)
        .await
        .map_err(ChatmailError::from)?;
    Ok(n.max(0) as u64)
}

async fn copy_one(src: &DbPool, dst: &mut PgConnection, table: &str, source: &str) -> Result<u64> {
    match table {
        "settings" => copy_settings(src, dst).await,
        "quotas" => copy_quotas(src, dst, source).await,
        "blocked_users" => copy_blocked_users(src, dst).await,
        "registration_tokens" => copy_registration_tokens(src, dst).await,
        "dns_overrides" => copy_dns_overrides(src, dst).await,
        "passwords" => copy_passwords(src, dst).await,
        "push_tokens" => copy_push_tokens(src, dst).await,
        "federation_rules" => copy_federation_rules(src, dst).await,
        "federation_server_stats" => copy_federation_stats(src, dst).await,
        "message_stats" => copy_message_stats(src, dst).await,
        "exchangers" => copy_exchangers(src, dst).await,
        "federation_silent_dismiss" => copy_silent_dismiss(src, dst).await,
        "mailbox_modseq" => copy_mailbox_modseq(src, dst).await,
        _ => Ok(0),
    }
}

async fn copy_settings(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    let rows: Vec<(String, String)> =
        crate::db_fetch_all!(src, (String, String), "SELECT key, value FROM settings")?;
    let n = rows.len() as u64;
    for (k, v) in rows {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES ($1, $2) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
        )
        .bind(k)
        .bind(v)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_quotas(src: &DbPool, dst: &mut PgConnection, source: &str) -> Result<u64> {
    let sql = format!(
        "SELECT username, max_storage, created_at, first_login_at, last_login_at, used_token \
         FROM {source}"
    );
    let rows: Vec<(String, i64, i64, i64, i64, Option<String>)> =
        crate::db_fetch_all!(src, (String, i64, i64, i64, i64, Option<String>), &sql)?;
    let n = rows.len() as u64;
    for (u, max, c, f, l, tok) in rows {
        sqlx::query(
            "INSERT INTO quotas (username, max_storage, created_at, first_login_at, last_login_at, used_token) \
             VALUES ($1,$2,$3,$4,$5,$6) \
             ON CONFLICT (username) DO UPDATE SET \
               max_storage = EXCLUDED.max_storage, \
               created_at = EXCLUDED.created_at, \
               first_login_at = EXCLUDED.first_login_at, \
               last_login_at = EXCLUDED.last_login_at, \
               used_token = EXCLUDED.used_token",
        )
        .bind(u)
        .bind(max)
        .bind(c)
        .bind(f)
        .bind(l)
        .bind(tok)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_blocked_users(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    let rows: Vec<(String, String, Option<String>)> = crate::db_fetch_all!(
        src,
        (String, String, Option<String>),
        "SELECT username, reason, CAST(blocked_at AS TEXT) FROM blocked_users"
    )?;
    let n = rows.len() as u64;
    for (u, reason, at) in rows {
        sqlx::query(
            "INSERT INTO blocked_users (username, reason, blocked_at) VALUES ($1,$2,$3::timestamp) \
             ON CONFLICT (username) DO UPDATE SET reason = EXCLUDED.reason, blocked_at = EXCLUDED.blocked_at",
        )
        .bind(u)
        .bind(reason)
        .bind(at)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

#[allow(clippy::type_complexity)]
async fn copy_registration_tokens(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    let rows: Vec<(
        String,
        i64,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = crate::db_fetch_all!(
        src,
        (
            String,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>
        ),
        "SELECT token, max_uses, used_count, comment, CAST(expires_at AS TEXT), CAST(created_at AS TEXT) FROM registration_tokens"
    )?;
    let n = rows.len() as u64;
    for (token, max_uses, used, comment, exp, created) in rows {
        sqlx::query(
            "INSERT INTO registration_tokens (token, max_uses, used_count, comment, expires_at, created_at) \
             VALUES ($1,$2,$3,$4,$5::timestamp,$6::timestamp) \
             ON CONFLICT (token) DO UPDATE SET \
               max_uses = EXCLUDED.max_uses, used_count = EXCLUDED.used_count, \
               comment = EXCLUDED.comment, expires_at = EXCLUDED.expires_at, created_at = EXCLUDED.created_at",
        )
        .bind(token)
        .bind(max_uses)
        .bind(used)
        .bind(comment)
        .bind(exp)
        .bind(created)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

#[allow(clippy::type_complexity)]
async fn copy_dns_overrides(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    let rows: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = crate::db_fetch_all!(
        src,
        (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>
        ),
        "SELECT lookup_key, target_host, comment, CAST(created_at AS TEXT), CAST(updated_at AS TEXT) FROM dns_overrides"
    )?;
    let n = rows.len() as u64;
    for (k, host, comment, c, u) in rows {
        sqlx::query(
            "INSERT INTO dns_overrides (lookup_key, target_host, comment, created_at, updated_at) \
             VALUES ($1,$2,$3,$4::timestamp,$5::timestamp) \
             ON CONFLICT (lookup_key) DO UPDATE SET \
               target_host = EXCLUDED.target_host, comment = EXCLUDED.comment, \
               created_at = EXCLUDED.created_at, updated_at = EXCLUDED.updated_at",
        )
        .bind(k)
        .bind(host)
        .bind(comment)
        .bind(c)
        .bind(u)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_passwords(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    match passwords_layout(src).await? {
        PasswordsLayout::MadmailKv => {
            let rows: Vec<(String, String)> =
                crate::db_fetch_all!(src, (String, String), "SELECT key, value FROM passwords")?;
            let n = rows.len() as u64;
            let now = unix_now();
            for (user, hash) in rows {
                sqlx::query(
                    "INSERT INTO passwords (username, hash, created_at) VALUES ($1,$2,$3) \
                     ON CONFLICT (username) DO UPDATE SET hash = EXCLUDED.hash",
                )
                .bind(user)
                .bind(hash)
                .bind(now)
                .execute(&mut *dst)
                .await
                .map_err(ChatmailError::from)?;
            }
            Ok(n)
        }
        PasswordsLayout::ChatmailRs => {
            let rows: Vec<(String, String, i64)> = crate::db_fetch_all!(
                src,
                (String, String, i64),
                "SELECT username, hash, created_at FROM passwords"
            )?;
            let n = rows.len() as u64;
            for (user, hash, created) in rows {
                sqlx::query(
                    "INSERT INTO passwords (username, hash, created_at) VALUES ($1,$2,$3) \
                     ON CONFLICT (username) DO UPDATE SET hash = EXCLUDED.hash, created_at = EXCLUDED.created_at",
                )
                .bind(user)
                .bind(hash)
                .bind(created)
                .execute(&mut *dst)
                .await
                .map_err(ChatmailError::from)?;
            }
            Ok(n)
        }
        PasswordsLayout::Unknown => Ok(0),
    }
}

async fn copy_push_tokens(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    let rows: Vec<(String, String, Option<String>)> = crate::db_fetch_all!(
        src,
        (String, String, Option<String>),
        "SELECT username, device_token, CAST(updated_at AS TEXT) FROM push_tokens"
    )?;
    let n = rows.len() as u64;
    for (u, tok, at) in rows {
        sqlx::query(
            "INSERT INTO push_tokens (username, device_token, updated_at) VALUES ($1,$2,$3::timestamp) \
             ON CONFLICT (username, device_token) DO UPDATE SET updated_at = EXCLUDED.updated_at",
        )
        .bind(u)
        .bind(tok)
        .bind(at)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_federation_rules(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    let DbPool::Sqlite(sp) = src else {
        return Ok(0);
    };
    let cols: Vec<String> = sqlx::query("PRAGMA table_info(federation_rules)")
        .fetch_all(sp)
        .await
        .map_err(ChatmailError::from)?
        .into_iter()
        .filter_map(|r| r.try_get::<String, _>("name").ok())
        .collect();
    let has_action = cols.iter().any(|c| c == "action");
    let sql = if has_action {
        "SELECT domain, CAST(strftime('%s', COALESCE(created_at, 'now')) AS INTEGER) FROM federation_rules"
    } else {
        "SELECT domain, created_at FROM federation_rules"
    };
    let rows: Vec<(String, i64)> = crate::db_fetch_all!(src, (String, i64), sql)?;
    let n = rows.len() as u64;
    for (domain, created) in rows {
        sqlx::query(
            "INSERT INTO federation_rules (domain, created_at) VALUES ($1,$2) \
             ON CONFLICT (domain) DO UPDATE SET created_at = EXCLUDED.created_at",
        )
        .bind(domain)
        .bind(created)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

#[allow(clippy::type_complexity)]
async fn copy_federation_stats(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    let rows: Vec<(
        String,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
    )> = {
        // Go Madmail names these failed_http_s / success_http_s (see
        // `schema::federation_stats_columns`); reading the v2 names blows up the
        // whole copy with "no such column" on a legacy database.
        let cols = federation_stats_columns(src).await?;
        let sql = format!(
            "SELECT domain, queued_messages, failed_http, {failed_https}, failed_smtp, \
                 success_http, {success_https}, success_smtp, inbound_deliveries, \
                 successful_deliveries, total_latency_ms, last_active FROM federation_server_stats",
            failed_https = cols.failed_https,
            success_https = cols.success_https,
        );
        crate::db_fetch_all!(
            src,
            (String, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64),
            &sql
        )?
    };
    let n = rows.len() as u64;
    for r in rows {
        sqlx::query(
            "INSERT INTO federation_server_stats (\
               domain, queued_messages, failed_http, failed_https, failed_smtp, \
               success_http, success_https, success_smtp, inbound_deliveries, \
               successful_deliveries, total_latency_ms, last_active\
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) \
             ON CONFLICT (domain) DO UPDATE SET \
               queued_messages = EXCLUDED.queued_messages, \
               failed_http = EXCLUDED.failed_http, failed_https = EXCLUDED.failed_https, \
               failed_smtp = EXCLUDED.failed_smtp, success_http = EXCLUDED.success_http, \
               success_https = EXCLUDED.success_https, success_smtp = EXCLUDED.success_smtp, \
               inbound_deliveries = EXCLUDED.inbound_deliveries, \
               successful_deliveries = EXCLUDED.successful_deliveries, \
               total_latency_ms = EXCLUDED.total_latency_ms, last_active = EXCLUDED.last_active",
        )
        .bind(r.0)
        .bind(r.1)
        .bind(r.2)
        .bind(r.3)
        .bind(r.4)
        .bind(r.5)
        .bind(r.6)
        .bind(r.7)
        .bind(r.8)
        .bind(r.9)
        .bind(r.10)
        .bind(r.11)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_message_stats(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    let rows: Vec<(String, i64)> =
        crate::db_fetch_all!(src, (String, i64), "SELECT name, count FROM message_stats")?;
    let n = rows.len() as u64;
    for (name, count) in rows {
        sqlx::query(
            "INSERT INTO message_stats (name, count) VALUES ($1,$2) \
             ON CONFLICT (name) DO UPDATE SET count = EXCLUDED.count",
        )
        .bind(name)
        .bind(count)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

#[allow(clippy::type_complexity)]
async fn copy_exchangers(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    let rows: Vec<(
        String,
        String,
        i64,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = crate::db_fetch_all!(
        src,
        (
            String,
            String,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>
        ),
        "SELECT name, url, enabled, poll_interval, CAST(last_poll_at AS TEXT), \
         CAST(created_at AS TEXT), CAST(updated_at AS TEXT) FROM exchangers"
    )?;
    let n = rows.len() as u64;
    for (name, url, enabled, poll, last, c, u) in rows {
        sqlx::query(
            "INSERT INTO exchangers (name, url, enabled, poll_interval, last_poll_at, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5::timestamp,$6::timestamp,$7::timestamp) \
             ON CONFLICT (name) DO UPDATE SET \
               url = EXCLUDED.url, enabled = EXCLUDED.enabled, poll_interval = EXCLUDED.poll_interval, \
               last_poll_at = EXCLUDED.last_poll_at, created_at = EXCLUDED.created_at, updated_at = EXCLUDED.updated_at",
        )
        .bind(name)
        .bind(url)
        .bind(enabled)
        .bind(poll)
        .bind(last)
        .bind(c)
        .bind(u)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_silent_dismiss(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    let rows: Vec<(String, i64)> = crate::db_fetch_all!(
        src,
        (String, i64),
        "SELECT domain, created_at FROM federation_silent_dismiss"
    )?;
    let n = rows.len() as u64;
    for (domain, created) in rows {
        sqlx::query(
            "INSERT INTO federation_silent_dismiss (domain, created_at) VALUES ($1,$2) \
             ON CONFLICT (domain) DO UPDATE SET created_at = EXCLUDED.created_at",
        )
        .bind(domain)
        .bind(created)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_mailbox_modseq(src: &DbPool, dst: &mut PgConnection) -> Result<u64> {
    let rows: Vec<(String, i64)> = crate::db_fetch_all!(
        src,
        (String, i64),
        "SELECT username, modseq FROM mailbox_modseq"
    )?;
    let n = rows.len() as u64;
    for (u, m) in rows {
        sqlx::query(
            "INSERT INTO mailbox_modseq (username, modseq) VALUES ($1,$2) \
             ON CONFLICT (username) DO UPDATE SET modseq = EXCLUDED.modseq",
        )
        .bind(u)
        .bind(m)
        .execute(&mut *dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_db;

    #[tokio::test]
    async fn inspect_counts_seeded_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chatmail.db");
        let pool = init_db(&path).await.unwrap();
        crate::set_setting(&pool, "__REGISTRATION_OPEN__", "true")
            .await
            .unwrap();
        crate::passwords::create_user(&pool, "alice@test", "hash:1")
            .await
            .unwrap();

        let tables = inspect_sqlite_tables(&path).await.unwrap();
        let settings = tables.iter().find(|t| t.table == "settings").unwrap();
        assert!(settings.sqlite_rows >= 1);
        let pw = tables.iter().find(|t| t.table == "passwords").unwrap();
        assert_eq!(pw.sqlite_rows, 1);
        assert!(!pw.skipped);
    }

    /// Go-era databases keep account rows in singular `quota`; v2 leaves an empty
    /// `quotas` next to it, so reading `quotas` would report 0 rows and look like success.
    #[tokio::test]
    async fn inspect_prefers_legacy_quota_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chatmail.db");
        let pool = init_db(&path).await.unwrap();
        crate::db_execute!(
            &pool,
            "CREATE TABLE quota (username TEXT PRIMARY KEY NOT NULL, max_storage BIGINT NOT NULL \
             DEFAULT 0, created_at BIGINT NOT NULL, first_login_at BIGINT NOT NULL, \
             last_login_at BIGINT NOT NULL, used_token TEXT)"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO quota (username, max_storage, created_at, first_login_at, last_login_at) \
             VALUES ('alice@test', 1, 2, 3, 4)"
        )
        .unwrap();

        let tables = inspect_sqlite_tables(&path).await.unwrap();
        let q = tables.iter().find(|t| t.table == "quotas").unwrap();
        assert_eq!(q.source_table, "quota");
        assert_eq!(q.sqlite_rows, 1);
        assert!(!q.skipped);
    }

    #[tokio::test]
    async fn missing_sqlite_file_errors() {
        let err = inspect_sqlite_tables(Path::new("/no/such/chatmail.db"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn dry_run_does_not_need_postgres() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chatmail.db");
        init_db(&path).await.unwrap();
        let report = copy_sqlite_to_postgres(
            &path,
            "postgres://unused",
            CopyOpts {
                dry_run: true,
                force: false,
            },
        )
        .await
        .unwrap();
        assert!(report.dry_run);
        assert!(!report.tables.is_empty());
    }

    /// Live dest-side tests. Skip unless `TEST_POSTGRES_DSN` is set (CI Test job).
    fn live_pg_dsn() -> Option<String> {
        match std::env::var("TEST_POSTGRES_DSN") {
            Ok(s) if !s.trim().is_empty() => Some(s),
            _ => {
                eprintln!("skipping live Postgres test (set TEST_POSTGRES_DSN)");
                None
            }
        }
    }

    fn dsn_with_database(base: &str, db: &str) -> String {
        let base = base.trim();
        if base.starts_with("postgres://") || base.starts_with("postgresql://") {
            let (path_part, query) = match base.split_once('?') {
                Some((p, q)) => (p, format!("?{q}")),
                None => (base, String::new()),
            };
            let prefix = path_part
                .rsplit_once('/')
                .map(|(h, _)| h)
                .unwrap_or(path_part);
            format!("{prefix}/{db}{query}")
        } else {
            let mut parts: Vec<String> = base
                .split_whitespace()
                .filter(|p| !p.starts_with("dbname="))
                .map(str::to_string)
                .collect();
            parts.push(format!("dbname={db}"));
            parts.join(" ")
        }
    }

    struct LivePg {
        admin: DbPool,
        dbname: String,
        dsn: String,
    }

    impl LivePg {
        async fn create() -> Option<Self> {
            let base = live_pg_dsn()?;
            let admin = connect_database(&DatabaseConfig {
                driver: DbDriver::Postgres,
                dsn: base.clone(),
            })
            .await
            .expect("TEST_POSTGRES_DSN must accept connections");
            static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let dbname = format!(
                "mm_t_{}_{}",
                std::process::id(),
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            let DbPool::Postgres(pg) = &admin else {
                panic!("admin DSN is not Postgres");
            };
            sqlx::query(&format!("CREATE DATABASE {dbname}"))
                .execute(pg)
                .await
                .unwrap_or_else(|e| panic!("CREATE DATABASE {dbname}: {e}"));
            Some(Self {
                admin,
                dsn: dsn_with_database(&base, &dbname),
                dbname,
            })
        }

        async fn connect(&self) -> DbPool {
            connect_database(&DatabaseConfig {
                driver: DbDriver::Postgres,
                dsn: self.dsn.clone(),
            })
            .await
            .expect("connect to isolated Postgres database")
        }

        async fn drop(self) {
            if let DbPool::Postgres(pg) = &self.admin {
                let _ = sqlx::query(&format!(
                    "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                    self.dbname
                ))
                .execute(pg)
                .await;
            }
        }
    }

    async fn seed_sqlite(path: &Path) {
        let pool = init_db(path).await.unwrap();
        crate::set_setting(&pool, "__REGISTRATION_OPEN__", "true")
            .await
            .unwrap();
        crate::passwords::create_user(&pool, "alice@test", "hash:alice")
            .await
            .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO quotas (username, max_storage, created_at, first_login_at, last_login_at, used_token)
             VALUES ('alice@test', 12345, 100, 200, 300, 'tok-used')"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO registration_tokens (token, max_uses, used_count, comment, expires_at)
             VALUES ('tok-live', 3, 0, 'live-pg', '2030-01-15 12:00:00')"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO exchangers (name, url, enabled, poll_interval, last_poll_at)
             VALUES ('ex1', 'https://ex.example/pull', 1, 60, '2030-06-01 08:30:00')"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO blocked_users (username, reason, blocked_at)
             VALUES ('bob@test', 'abuse', '2029-12-01 00:00:00')"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO dns_overrides (lookup_key, target_host, comment, created_at, updated_at)
             VALUES ('mx.example', '127.0.0.1', 'lab', '2028-03-01 00:00:00', '2028-03-02 00:00:00')"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO push_tokens (username, device_token, updated_at)
             VALUES ('alice@test', 'dev-token', '2027-04-01 00:00:00')"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO federation_rules (domain, created_at) VALUES ('blocked.example', 1700000000)"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO federation_server_stats (
                domain, queued_messages, failed_http, failed_https, failed_smtp,
                success_http, success_https, success_smtp, inbound_deliveries,
                successful_deliveries, total_latency_ms, last_active
             ) VALUES ('peer.example', 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11)"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "UPDATE message_stats SET count = 42 WHERE name = 'sent_messages'"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO federation_silent_dismiss (domain, created_at)
             VALUES ('silent.example', 1700000001)"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO mailbox_modseq (username, modseq) VALUES ('alice@test', 99)"
        )
        .unwrap();
    }

    fn table<'a>(report: &'a CopyReport, name: &str) -> &'a TableCopy {
        report
            .tables
            .iter()
            .find(|t| t.table == name)
            .unwrap_or_else(|| panic!("missing {name} in copy report"))
    }

    async fn text_col(pool: &DbPool, sql: &str, bind: &str) -> String {
        let row: Option<(String,)> = crate::db_fetch_optional!(pool, (String,), sql, bind).unwrap();
        row.map(|(v,)| v)
            .unwrap_or_else(|| panic!("no row for {sql} bind={bind}"))
    }

    async fn i64_col(pool: &DbPool, sql: &str, bind: &str) -> i64 {
        let row: Option<(i64,)> = crate::db_fetch_optional!(pool, (i64,), sql, bind).unwrap();
        row.map(|(v,)| v)
            .unwrap_or_else(|| panic!("no row for {sql} bind={bind}"))
    }

    #[test]
    fn dsn_with_database_rewrites_url_and_libpq() {
        assert_eq!(
            dsn_with_database(
                "postgres://u:p@127.0.0.1:5432/madmail_test?sslmode=disable",
                "mm_t_1"
            ),
            "postgres://u:p@127.0.0.1:5432/mm_t_1?sslmode=disable"
        );
        assert_eq!(
            dsn_with_database("host=127.0.0.1 user=madmail dbname=madmail_test", "mm_t_1"),
            "host=127.0.0.1 user=madmail dbname=mm_t_1"
        );
    }

    #[tokio::test]
    async fn copy_refuses_nonempty_postgres_without_force() {
        let Some(pg) = LivePg::create().await else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chatmail.db");
        seed_sqlite(&path).await;

        let dest = pg.connect().await;
        run_migrations(&dest).await.unwrap();
        crate::set_setting(&dest, "sentinel", "keep").await.unwrap();

        let err = copy_sqlite_to_postgres(
            &path,
            &pg.dsn,
            CopyOpts {
                dry_run: false,
                force: false,
            },
        )
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("already has rows") && msg.contains("settings"),
            "{msg}"
        );

        let kept: Option<(String,)> = crate::db_fetch_optional!(
            &dest,
            (String,),
            "SELECT value FROM settings WHERE key = ?",
            "sentinel"
        )
        .unwrap();
        assert_eq!(kept.as_ref().map(|(v,)| v.as_str()), Some("keep"));
        let alice: Option<(String,)> = crate::db_fetch_optional!(
            &dest,
            (String,),
            "SELECT hash FROM passwords WHERE username = ?",
            "alice@test"
        )
        .unwrap();
        assert!(alice.is_none(), "refuse must not copy passwords");
        pg.drop().await;
    }

    #[tokio::test]
    async fn copy_refuses_legacy_kv_passwords_before_delete() {
        let Some(pg) = LivePg::create().await else {
            return;
        };
        let dest = pg.connect().await;
        crate::db_execute!(
            &dest,
            "CREATE TABLE passwords (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)"
        )
        .unwrap();
        crate::db_execute!(
            &dest,
            "INSERT INTO passwords (key, value) VALUES ('keep-me', 'legacy-hash')"
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chatmail.db");
        seed_sqlite(&path).await;

        let err = copy_sqlite_to_postgres(
            &path,
            &pg.dsn,
            CopyOpts {
                dry_run: false,
                force: true,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("key/value layout"), "{}", err);

        let kept: Option<(String,)> = crate::db_fetch_optional!(
            &dest,
            (String,),
            "SELECT value FROM passwords WHERE key = ?",
            "keep-me"
        )
        .unwrap();
        assert_eq!(
            kept.as_ref().map(|(v,)| v.as_str()),
            Some("legacy-hash"),
            "KV dest must not be emptied before the layout refusal"
        );
        pg.drop().await;
    }

    #[tokio::test]
    async fn copy_roundtrip_settings_passwords_and_timestamps() {
        let Some(pg) = LivePg::create().await else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chatmail.db");
        seed_sqlite(&path).await;

        let report = copy_sqlite_to_postgres(
            &path,
            &pg.dsn,
            CopyOpts {
                dry_run: false,
                force: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(report.tables.len(), COPY_TABLES.len());
        for name in COPY_TABLES {
            let t = table(&report, name);
            assert!(!t.skipped, "{name} skipped");
            assert!(t.copied > 0, "{name} copied 0 of {}", t.sqlite_rows);
        }

        let dest = pg.connect().await;
        assert_eq!(
            text_col(
                &dest,
                "SELECT value FROM settings WHERE key = ?",
                "__REGISTRATION_OPEN__"
            )
            .await,
            "true"
        );
        assert_eq!(
            text_col(
                &dest,
                "SELECT hash FROM passwords WHERE username = ?",
                "alice@test"
            )
            .await,
            "hash:alice"
        );
        assert_eq!(
            i64_col(
                &dest,
                "SELECT max_storage FROM quotas WHERE username = ?",
                "alice@test"
            )
            .await,
            12345
        );
        assert_eq!(
            text_col(
                &dest,
                "SELECT used_token FROM quotas WHERE username = ?",
                "alice@test"
            )
            .await,
            "tok-used"
        );
        let blocked_at = text_col(
            &dest,
            "SELECT CAST(blocked_at AS TEXT) FROM blocked_users WHERE username = ?",
            "bob@test",
        )
        .await;
        assert!(
            blocked_at.contains("2029-12-01"),
            "blocked_at round-trip, got {blocked_at}"
        );
        let expires = text_col(
            &dest,
            "SELECT CAST(expires_at AS TEXT) FROM registration_tokens WHERE token = ?",
            "tok-live",
        )
        .await;
        assert!(
            expires.contains("2030-01-15"),
            "expires_at round-trip, got {expires}"
        );
        let dns_created = text_col(
            &dest,
            "SELECT CAST(created_at AS TEXT) FROM dns_overrides WHERE lookup_key = ?",
            "mx.example",
        )
        .await;
        assert!(
            dns_created.contains("2028-03-01"),
            "dns created_at round-trip, got {dns_created}"
        );
        let push_at = text_col(
            &dest,
            "SELECT CAST(updated_at AS TEXT) FROM push_tokens WHERE username = ?",
            "alice@test",
        )
        .await;
        assert!(
            push_at.contains("2027-04-01"),
            "push updated_at round-trip, got {push_at}"
        );
        assert_eq!(
            i64_col(
                &dest,
                "SELECT created_at FROM federation_rules WHERE domain = ?",
                "blocked.example"
            )
            .await,
            1_700_000_000
        );
        assert_eq!(
            i64_col(
                &dest,
                "SELECT CAST(failed_https AS BIGINT) FROM federation_server_stats WHERE domain = ?",
                "peer.example"
            )
            .await,
            3
        );
        assert_eq!(
            i64_col(
                &dest,
                "SELECT count FROM message_stats WHERE name = ?",
                "sent_messages"
            )
            .await,
            42
        );
        let poll = text_col(
            &dest,
            "SELECT CAST(last_poll_at AS TEXT) FROM exchangers WHERE name = ?",
            "ex1",
        )
        .await;
        assert!(
            poll.contains("2030-06-01"),
            "last_poll_at round-trip, got {poll}"
        );
        assert_eq!(
            i64_col(
                &dest,
                "SELECT created_at FROM federation_silent_dismiss WHERE domain = ?",
                "silent.example"
            )
            .await,
            1_700_000_001
        );
        assert_eq!(
            i64_col(
                &dest,
                "SELECT modseq FROM mailbox_modseq WHERE username = ?",
                "alice@test"
            )
            .await,
            99
        );

        let sql = format!(
            "INSERT INTO registration_tokens (token, max_uses, used_count, comment, expires_at)
             VALUES (?, ?, 0, ?, {})",
            dest.timestamp_param()
        );
        crate::db_execute!(
            &dest,
            &sql,
            "tok-write",
            1i64,
            "write-path",
            Some("2031-02-01 00:00:00")
        )
        .unwrap();
        let written = text_col(
            &dest,
            "SELECT CAST(expires_at AS TEXT) FROM registration_tokens WHERE token = ?",
            "tok-write",
        )
        .await;
        assert!(
            written.contains("2031-02-01"),
            "timestamp_param write, got {written}"
        );
        pg.drop().await;
    }

    #[tokio::test]
    async fn copy_force_replaces_existing_postgres_rows() {
        let Some(pg) = LivePg::create().await else {
            return;
        };
        let dest = pg.connect().await;
        run_migrations(&dest).await.unwrap();
        crate::set_setting(&dest, "sentinel", "keep").await.unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chatmail.db");
        seed_sqlite(&path).await;

        copy_sqlite_to_postgres(
            &path,
            &pg.dsn,
            CopyOpts {
                dry_run: false,
                force: true,
            },
        )
        .await
        .unwrap();

        let gone: Option<(String,)> = crate::db_fetch_optional!(
            &dest,
            (String,),
            "SELECT value FROM settings WHERE key = ?",
            "sentinel"
        )
        .unwrap();
        assert!(gone.is_none(), "force must delete dest-only rows");
        assert_eq!(
            text_col(
                &dest,
                "SELECT hash FROM passwords WHERE username = ?",
                "alice@test"
            )
            .await,
            "hash:alice"
        );
        assert_eq!(
            i64_col(
                &dest,
                "SELECT count FROM message_stats WHERE name = ?",
                "sent_messages"
            )
            .await,
            42
        );
        pg.drop().await;
    }

    #[tokio::test]
    async fn copy_legacy_sqlite_quota_into_postgres_quotas() {
        let Some(pg) = LivePg::create().await else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chatmail.db");
        let pool = init_db(&path).await.unwrap();
        crate::db_execute!(
            &pool,
            "CREATE TABLE quota (username TEXT PRIMARY KEY NOT NULL, max_storage BIGINT NOT NULL \
             DEFAULT 0, created_at BIGINT NOT NULL, first_login_at BIGINT NOT NULL, \
             last_login_at BIGINT NOT NULL, used_token TEXT)"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO quota (username, max_storage, created_at, first_login_at, last_login_at, used_token) \
             VALUES ('legacy@test', 999, 1, 2, 3, 'from-quota')"
        )
        .unwrap();

        let report = copy_sqlite_to_postgres(
            &path,
            &pg.dsn,
            CopyOpts {
                dry_run: false,
                force: false,
            },
        )
        .await
        .unwrap();
        let q = table(&report, "quotas");
        assert_eq!(q.source_table, "quota");
        assert_eq!(q.copied, 1);

        let dest = pg.connect().await;
        assert_eq!(
            i64_col(
                &dest,
                "SELECT max_storage FROM quotas WHERE username = ?",
                "legacy@test"
            )
            .await,
            999
        );
        assert_eq!(
            text_col(
                &dest,
                "SELECT used_token FROM quotas WHERE username = ?",
                "legacy@test"
            )
            .await,
            "from-quota"
        );
        pg.drop().await;
    }

    #[tokio::test]
    async fn copy_legacy_federation_http_s_columns() {
        let Some(pg) = LivePg::create().await else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chatmail.db");
        let pool = init_db(&path).await.unwrap();
        crate::db_execute!(&pool, "DROP TABLE federation_server_stats").unwrap();
        crate::db_execute!(
            &pool,
            "CREATE TABLE federation_server_stats (
                domain TEXT PRIMARY KEY NOT NULL,
                queued_messages INTEGER NOT NULL DEFAULT 0,
                failed_http INTEGER NOT NULL DEFAULT 0,
                failed_http_s INTEGER NOT NULL DEFAULT 0,
                failed_smtp INTEGER NOT NULL DEFAULT 0,
                success_http INTEGER NOT NULL DEFAULT 0,
                success_http_s INTEGER NOT NULL DEFAULT 0,
                success_smtp INTEGER NOT NULL DEFAULT 0,
                inbound_deliveries INTEGER NOT NULL DEFAULT 0,
                successful_deliveries INTEGER NOT NULL DEFAULT 0,
                total_latency_ms INTEGER NOT NULL DEFAULT 0,
                last_active INTEGER NOT NULL DEFAULT 0
            )"
        )
        .unwrap();
        crate::db_execute!(
            &pool,
            "INSERT INTO federation_server_stats (
                domain, queued_messages, failed_http, failed_http_s, failed_smtp,
                success_http, success_http_s, success_smtp, inbound_deliveries,
                successful_deliveries, total_latency_ms, last_active
             ) VALUES ('go-peer.example', 0, 0, 17, 0, 0, 19, 0, 0, 0, 0, 0)"
        )
        .unwrap();

        let report = copy_sqlite_to_postgres(
            &path,
            &pg.dsn,
            CopyOpts {
                dry_run: false,
                force: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(table(&report, "federation_server_stats").copied, 1);

        let dest = pg.connect().await;
        assert_eq!(
            i64_col(
                &dest,
                "SELECT CAST(failed_https AS BIGINT) FROM federation_server_stats WHERE domain = ?",
                "go-peer.example"
            )
            .await,
            17
        );
        assert_eq!(
            i64_col(
                &dest,
                "SELECT CAST(success_https AS BIGINT) FROM federation_server_stats WHERE domain = ?",
                "go-peer.example"
            )
            .await,
            19
        );
        pg.drop().await;
    }
}
