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

//! SQLite migration compatibility for databases created by LF and CRLF builds.

use std::borrow::Cow;

use sqlx::migrate::{MigrateError, Migration, Migrator};
use sqlx::sqlite::SqlitePool;
use sqlx::SqlSafeStr;

// SHA-384 of the six immutable SQLite migrations shipped through v2.44.2, with LF.
// A changed historical SQL file must not expand the set of accepted CRLF hashes.
const RELEASED_LF: &[(i64, &str)] = &[
    (20240101000000, "aca447fdf86c21b22a708c5b380db0daf0830492aa85fe4d6a720259d10d53bae7c405de27fde3bbd27045dcb8de3f8f"),
    (20240201000000, "dd875e60154a0ac8bc406e861df1accab833f86ce33f681fde2374cf68423d728130a1b9c3e55a4f9f3f01aa2c936cb6"),
    (20240301000000, "e4da1b6b5d9c07418add33e546c38f8be21b5a19696f70605195e8f6f9dde203241e50ffda151bfd766ea33f0ce655fd"),
    (20240401000000, "58b578c0b21938d9e61af5d5d4935f1c709fc69b0050961619d9d70b6e45ce660c8c599642d5fe84c0e4fd558d235d74"),
    (20240501000000, "260be6d85def22c0bcd61773f9c61703c79b671c5a065d8682d6c950b79410c196a3e54ae4364c4b98dcb4871deb1939"),
    (20240601000000, "2546be813b496bdc82a43f3ee48c55561f4fd64dde71d4eb48dddc83e614b7e216a6935554cd6bfb56a248a1155534f3"),
];

fn canonicalize(mut source: Migrator) -> Result<Migrator, MigrateError> {
    source.migrations = Cow::Owned(
        source
            .iter()
            .map(|m| {
                // Only embedded SQL is normalized; no external text is incorporated.
                Migration::new(
                    m.version,
                    m.description.clone(),
                    m.migration_type,
                    sqlx::AssertSqlSafe(m.sql.as_str().replace("\r\n", "\n")).into_sql_str(),
                    m.no_tx,
                )
            })
            .collect(),
    );
    for (version, pinned) in RELEASED_LF {
        let m = source
            .iter()
            .find(|m| m.version == *version)
            .ok_or(MigrateError::VersionMissing(*version))?;
        let hex: String = m.checksum.iter().map(|b| format!("{b:02x}")).collect();
        if hex != *pinned {
            return Err(MigrateError::VersionMismatch(*version));
        }
    }
    Ok(source)
}

pub(crate) async fn run(pool: &SqlitePool) -> Result<(), MigrateError> {
    let mut migrator = canonicalize(sqlx::migrate!("./migrations/sqlite"))?;
    let exists: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    if exists != 0 {
        let applied: Vec<(i64, bool, Vec<u8>)> = sqlx::query_as(
            "SELECT version,success,checksum FROM _sqlx_migrations ORDER BY version",
        )
        .fetch_all(pool)
        .await?;
        // Validate the complete history before SQLx can apply any pending migration.
        for (version, success, checksum) in applied {
            if !success {
                return Err(MigrateError::Dirty(version));
            }
            let m = migrator
                .migrations
                .to_mut()
                .iter_mut()
                .find(|m| m.version == version)
                .ok_or(MigrateError::VersionMissing(version))?;
            if m.checksum.as_ref() == checksum.as_slice() {
                continue;
            }
            if !RELEASED_LF.iter().any(|(v, _)| *v == version) {
                return Err(MigrateError::VersionMismatch(version));
            }
            let historical = Migration::new(
                m.version,
                m.description.clone(),
                m.migration_type,
                sqlx::AssertSqlSafe(m.sql.as_str().replace('\n', "\r\n")).into_sql_str(),
                m.no_tx,
            );
            if historical.checksum.as_ref() != checksum.as_slice() {
                return Err(MigrateError::VersionMismatch(version));
            }
            // Match this known applied checksum in memory. Do not rewrite the database,
            // mark a migration successful, or re-execute SQL that already ran.
            m.checksum = Cow::Owned(checksum);
        }
    }
    // Retain SQLx's normal locking, dirty-history checks, and checksum validation.
    migrator.run(pool).await
}

#[cfg(test)]
mod tests {
    use super::canonicalize;
    use crate::pool::{run_migrations, DbPool};
    use sqlx::migrate::{Migration, Migrator};
    use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
    use sqlx::SqlSafeStr;

    type History = Vec<(i64, String, String, bool, Vec<u8>, i64)>;

    fn source_with_endings(endings: &[bool]) -> Migrator {
        let source = sqlx::migrate!("./migrations/sqlite");
        Migrator::with_migrations(
            source
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let lf = m.sql.as_str().replace("\r\n", "\n");
                    let sql = if endings[i % endings.len()] {
                        lf.replace('\n', "\r\n")
                    } else {
                        lf
                    };
                    Migration::new(
                        m.version,
                        m.description.clone(),
                        m.migration_type,
                        sqlx::AssertSqlSafe(sql).into_sql_str(),
                        m.no_tx,
                    )
                })
                .collect(),
        )
    }

    async fn fixture(endings: &[bool], through: Option<i64>) -> SqlitePool {
        let p = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let m = source_with_endings(endings);
        if let Some(v) = through {
            m.run_to(v, &p).await.unwrap();
        } else {
            m.run(&p).await.unwrap();
        }
        sqlx::query("INSERT INTO settings(key,value) VALUES ('compat_fixture','preserve exactly')")
            .execute(&p)
            .await
            .unwrap();
        sqlx::query("INSERT INTO passwords(username,hash) VALUES ('fixture@example.invalid','not-a-real-password-hash')").execute(&p).await.unwrap();
        p
    }

    async fn history(p: &SqlitePool) -> History {
        sqlx::query_as("SELECT version,description,CAST(installed_on AS TEXT),success,checksum,execution_time FROM _sqlx_migrations ORDER BY version").fetch_all(p).await.unwrap()
    }

    async fn preserved(p: &SqlitePool) {
        let v: String = sqlx::query_scalar("SELECT value FROM settings WHERE key='compat_fixture'")
            .fetch_one(p)
            .await
            .unwrap();
        assert_eq!(v, "preserve exactly");
        let v: String = sqlx::query_scalar(
            "SELECT hash FROM passwords WHERE username='fixture@example.invalid'",
        )
        .fetch_one(p)
        .await
        .unwrap();
        assert_eq!(v, "not-a-real-password-hash");
    }

    #[tokio::test]
    async fn lf_crlf_and_mixed_history_are_preserved_on_repeated_startup() {
        for endings in [vec![false], vec![true], vec![false, true]] {
            let p = fixture(&endings, None).await;
            let before = history(&p).await;
            let db = DbPool::Sqlite(p.clone());
            run_migrations(&db).await.unwrap();
            run_migrations(&db).await.unwrap();
            assert_eq!(history(&p).await, before);
            preserved(&p).await;
        }
    }

    #[tokio::test]
    async fn unknown_checksum_is_rejected_before_pending_migrations() {
        for version in [20240101000000_i64, 20240501000000] {
            let p = fixture(&[true], Some(20240501000000)).await;
            sqlx::query("UPDATE _sqlx_migrations SET checksum=zeroblob(48) WHERE version=?")
                .bind(version)
                .execute(&p)
                .await
                .unwrap();
            let before = history(&p).await;
            let err = run_migrations(&DbPool::Sqlite(p.clone()))
                .await
                .unwrap_err();
            assert!(err.to_string().contains("checksum"), "{err}");
            assert_eq!(history(&p).await, before);
            let exists: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='mailbox_modseq'",
            )
            .fetch_one(&p)
            .await
            .unwrap();
            assert_eq!(exists, 0);
            preserved(&p).await;
        }
    }

    #[tokio::test]
    async fn failed_migration_is_not_repaired_or_marked_successful() {
        let p = fixture(&[true], None).await;
        sqlx::query("UPDATE _sqlx_migrations SET success=0 WHERE version=20240301000000")
            .execute(&p)
            .await
            .unwrap();
        let before = history(&p).await;
        assert!(run_migrations(&DbPool::Sqlite(p.clone())).await.is_err());
        assert_eq!(history(&p).await, before);
        preserved(&p).await;
    }

    #[tokio::test]
    async fn unknown_migration_version_is_rejected() {
        let p = fixture(&[true], None).await;
        sqlx::query("INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES(99999999999999,'unknown',1,zeroblob(48),0)").execute(&p).await.unwrap();
        let before = history(&p).await;
        assert!(run_migrations(&DbPool::Sqlite(p.clone())).await.is_err());
        assert_eq!(history(&p).await, before);
    }

    #[tokio::test]
    async fn pending_migrations_use_lf_without_rewriting_crlf_history() {
        let p = fixture(&[true], Some(20240301000000)).await;
        let before = history(&p).await;
        run_migrations(&DbPool::Sqlite(p.clone())).await.unwrap();
        let after = history(&p).await;
        assert_eq!(&after[..before.len()], before.as_slice());
        let lf = source_with_endings(&[false]);
        for row in &after[before.len()..] {
            assert_eq!(
                row.4.as_slice(),
                lf.iter()
                    .find(|m| m.version == row.0)
                    .unwrap()
                    .checksum
                    .as_ref()
            );
        }
        assert_eq!(after.len(), 6);
        preserved(&p).await;
    }

    #[tokio::test]
    async fn checksum_error_has_production_safe_guidance() {
        let p = fixture(&[false], None).await;
        sqlx::query(
            "UPDATE _sqlx_migrations SET checksum=zeroblob(48) WHERE version=20240101000000",
        )
        .execute(&p)
        .await
        .unwrap();
        let err = run_migrations(&DbPool::Sqlite(p))
            .await
            .unwrap_err()
            .to_string();
        assert!(!err.contains("make reset-db"), "{err}");
        assert!(err.contains("backup"), "{err}");
    }

    #[test]
    fn checkout_line_endings_produce_the_same_canonical_migrations() {
        let lf = canonicalize(source_with_endings(&[false])).unwrap();
        let crlf = canonicalize(source_with_endings(&[true])).unwrap();
        for (a, b) in lf.iter().zip(crlf.iter()) {
            assert_eq!(a.checksum, b.checksum);
            assert_eq!(a.sql.as_str(), b.sql.as_str());
            assert!(!a.sql.as_str().contains('\r'));
        }
    }

    #[test]
    fn editing_a_shipped_migration_does_not_expand_the_allowlist() {
        let mut source = source_with_endings(&[true]);
        let first = &mut source.migrations.to_mut()[0];
        first.sql =
            sqlx::AssertSqlSafe(format!("{}-- changed SQL\r\n", first.sql.as_str())).into_sql_str();
        assert!(canonicalize(source).is_err());
    }

    #[tokio::test]
    async fn fresh_database_always_records_canonical_lf() {
        let p = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        run_migrations(&DbPool::Sqlite(p.clone())).await.unwrap();
        let rows = history(&p).await;
        let lf = source_with_endings(&[false]);
        assert_eq!(rows.len(), 6);
        for row in rows {
            assert_eq!(
                row.4.as_slice(),
                lf.iter()
                    .find(|m| m.version == row.0)
                    .unwrap()
                    .checksum
                    .as_ref()
            );
        }
    }
}
