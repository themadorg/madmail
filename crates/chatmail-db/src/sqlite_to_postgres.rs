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
use sqlx::Row;

use crate::pool::{connect_database, run_migrations, DbPool};
use crate::schema::{passwords_layout, table_exists, PasswordsLayout};

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
    /// Replace existing Postgres rows (otherwise refuse if `passwords` is non-empty).
    pub force: bool,
}

#[derive(Debug, Clone)]
pub struct TableCopy {
    pub table: String,
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
    let mut tables = Vec::new();
    for name in COPY_TABLES {
        let exists = table_exists(&src, name).await?;
        let n = if exists {
            count_rows(&src, name).await?
        } else {
            0
        };
        tables.push(TableCopy {
            table: (*name).to_string(),
            sqlite_rows: n,
            copied: 0,
            skipped: !exists,
        });
    }
    Ok(tables)
}

pub async fn copy_sqlite_to_postgres(
    sqlite_path: &Path,
    postgres_dsn: &str,
    opts: CopyOpts,
) -> Result<CopyReport> {
    let sqlite_path_s = sqlite_path.display().to_string();
    let src = open_sqlite(sqlite_path).await?;

    if opts.dry_run {
        let tables = inspect_sqlite_tables(sqlite_path).await?;
        return Ok(CopyReport {
            sqlite_path: sqlite_path_s,
            dry_run: true,
            force: opts.force,
            tables,
        });
    }

    let dst_pool = connect_database(&DatabaseConfig {
        driver: DbDriver::Postgres,
        dsn: postgres_dsn.to_string(),
    })
    .await?;
    run_migrations(&dst_pool).await?;
    let dst = pg_pool(&dst_pool)?;

    let pw_count = count_pg(dst, "passwords").await?;
    if pw_count > 0 && !opts.force {
        return Err(ChatmailError::config(format!(
            "Postgres passwords table already has {pw_count} row(s). \
             Refusing to overwrite (pass --force to replace)."
        )));
    }

    if opts.force {
        for name in COPY_TABLES.iter().rev() {
            sqlx::query(&format!("DELETE FROM {name}"))
                .execute(dst)
                .await
                .map_err(ChatmailError::from)?;
        }
    }

    let mut tables = Vec::new();
    for name in COPY_TABLES {
        let exists = table_exists(&src, name).await?;
        if !exists {
            tables.push(TableCopy {
                table: (*name).to_string(),
                sqlite_rows: 0,
                copied: 0,
                skipped: true,
            });
            continue;
        }
        let sqlite_rows = count_rows(&src, name).await?;
        let copied = copy_one(&src, dst, name).await?;
        tables.push(TableCopy {
            table: (*name).to_string(),
            sqlite_rows,
            copied,
            skipped: false,
        });
    }

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

async fn count_pg(pool: &PgPool, table: &str) -> Result<u64> {
    let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .map_err(ChatmailError::from)?;
    Ok(n.max(0) as u64)
}

async fn copy_one(src: &DbPool, dst: &PgPool, table: &str) -> Result<u64> {
    match table {
        "settings" => copy_settings(src, dst).await,
        "quotas" => copy_quotas(src, dst).await,
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

async fn copy_settings(src: &DbPool, dst: &PgPool) -> Result<u64> {
    let rows: Vec<(String, String)> = crate::db_fetch_all!(
        src,
        (String, String),
        "SELECT key, value FROM settings"
    )?;
    let n = rows.len() as u64;
    for (k, v) in rows {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES ($1, $2) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
        )
        .bind(k)
        .bind(v)
        .execute(dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_quotas(src: &DbPool, dst: &PgPool) -> Result<u64> {
    let rows: Vec<(String, i64, i64, i64, i64, Option<String>)> = crate::db_fetch_all!(
        src,
        (String, i64, i64, i64, i64, Option<String>),
        "SELECT username, max_storage, created_at, first_login_at, last_login_at, used_token FROM quotas"
    )?;
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
        .execute(dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_blocked_users(src: &DbPool, dst: &PgPool) -> Result<u64> {
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
        .execute(dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

#[allow(clippy::type_complexity)]
async fn copy_registration_tokens(src: &DbPool, dst: &PgPool) -> Result<u64> {
    let rows: Vec<(
        String,
        i64,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = crate::db_fetch_all!(
        src,
        (String, i64, i64, Option<String>, Option<String>, Option<String>),
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
        .execute(dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

#[allow(clippy::type_complexity)]
async fn copy_dns_overrides(src: &DbPool, dst: &PgPool) -> Result<u64> {
    let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>)> =
        crate::db_fetch_all!(
            src,
            (String, String, Option<String>, Option<String>, Option<String>),
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
        .execute(dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_passwords(src: &DbPool, dst: &PgPool) -> Result<u64> {
    match passwords_layout(src).await? {
        PasswordsLayout::MadmailKv => {
            let rows: Vec<(String, String)> = crate::db_fetch_all!(
                src,
                (String, String),
                "SELECT key, value FROM passwords"
            )?;
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
                .execute(dst)
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
                .execute(dst)
                .await
                .map_err(ChatmailError::from)?;
            }
            Ok(n)
        }
        PasswordsLayout::Unknown => Ok(0),
    }
}

async fn copy_push_tokens(src: &DbPool, dst: &PgPool) -> Result<u64> {
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
        .execute(dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_federation_rules(src: &DbPool, dst: &PgPool) -> Result<u64> {
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
        .execute(dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

#[allow(clippy::type_complexity)]
async fn copy_federation_stats(src: &DbPool, dst: &PgPool) -> Result<u64> {
    let rows: Vec<(String, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64)> =
        crate::db_fetch_all!(
            src,
            (String, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64),
            "SELECT domain, queued_messages, failed_http, failed_https, failed_smtp, \
             success_http, success_https, success_smtp, inbound_deliveries, \
             successful_deliveries, total_latency_ms, last_active FROM federation_server_stats"
        )?;
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
        .execute(dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_message_stats(src: &DbPool, dst: &PgPool) -> Result<u64> {
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
        .execute(dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

#[allow(clippy::type_complexity)]
async fn copy_exchangers(src: &DbPool, dst: &PgPool) -> Result<u64> {
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
        (String, String, i64, i64, Option<String>, Option<String>, Option<String>),
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
        .execute(dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_silent_dismiss(src: &DbPool, dst: &PgPool) -> Result<u64> {
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
        .execute(dst)
        .await
        .map_err(ChatmailError::from)?;
    }
    Ok(n)
}

async fn copy_mailbox_modseq(src: &DbPool, dst: &PgPool) -> Result<u64> {
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
        .execute(dst)
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
}
