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

//! `madmail db` — SQLite → Postgres copy.

use std::path::PathBuf;

use chatmail_config::cli::DbCommand;
use chatmail_config::{effective_database_config, Args, DbDriver};
use chatmail_db::{copy_sqlite_to_postgres, CopyOpts};
use chatmail_types::{ChatmailError, Result};
use serde_json::json;

use super::context::CtlContext;
use super::output::CtlOut;
use super::util::confirm;

pub async fn db(args: &Args, cmd: &DbCommand) -> Result<()> {
    match cmd {
        DbCommand::SqliteToPostgres {
            dsn,
            sqlite,
            dry_run,
            force,
            yes,
        } => sqlite_to_postgres(args, dsn, sqlite.clone(), *dry_run, *force, *yes).await,
    }
}

async fn sqlite_to_postgres(
    args: &Args,
    dsn: &str,
    sqlite: Option<PathBuf>,
    dry_run: bool,
    force: bool,
    yes: bool,
) -> Result<()> {
    let out = CtlOut::from_args(args, "db sqlite-to-postgres");
    let ctx = CtlContext::from_args(args)?;
    let db = effective_database_config(&ctx.state_dir, &ctx.config);
    let sqlite_path = match sqlite {
        Some(p) => p,
        None => {
            if db.driver != DbDriver::Sqlite3 {
                return Err(ChatmailError::config(
                    "config application database is not SQLite; pass --sqlite PATH",
                ));
            }
            PathBuf::from(&db.dsn)
        }
    };

    if !dry_run
        && !confirm(
            &format!(
                "Copy {} into Postgres and (unless --force) refuse if passwords already exist?",
                sqlite_path.display()
            ),
            yes,
        )?
    {
        return out.aborted();
    }

    let report = copy_sqlite_to_postgres(
        &sqlite_path,
        dsn,
        CopyOpts { dry_run, force },
    )
    .await?;

    let tables: Vec<_> = report
        .tables
        .iter()
        .map(|t| {
            json!({
                "table": t.table,
                "sqlite_rows": t.sqlite_rows,
                "copied": t.copied,
                "skipped": t.skipped,
            })
        })
        .collect();
    let data = json!({
        "sqlite_path": report.sqlite_path,
        "dry_run": report.dry_run,
        "force": report.force,
        "tables": tables,
    });

    if out.is_json() {
        return out.emit(data);
    }

    out.blank();
    if dry_run {
        out.line("  SQLite → Postgres (dry run — no writes)");
    } else {
        out.line("  SQLite → Postgres copy finished");
    }
    out.blank();
    out.line(format!("  SQLite:  {}", report.sqlite_path));
    out.line("  TABLE\tSQLITE\tCOPIED\tSKIPPED");
    for t in &report.tables {
        out.line(format!(
            "  {}\t{}\t{}\t{}",
            t.table,
            t.sqlite_rows,
            t.copied,
            if t.skipped { "yes" } else { "no" }
        ));
    }
    out.blank();
    if !dry_run {
        out.line("  Point auth.pass_table and storage.imapsql at this DSN (driver postgres),");
        out.line("  then restart madmail. Mail, sharing.db, and admin_token are unchanged.");
        out.line("  See docs/project/user-guide/18-postgres.md");
        out.blank();
    }
    Ok(())
}
