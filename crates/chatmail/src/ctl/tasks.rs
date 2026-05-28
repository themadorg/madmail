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

//! `chatmail tasks` — run scheduled maintenance jobs on demand.

use chatmail_config::{Args, TasksCommand};
use chatmail_storage::MailboxStore;
use chatmail_tasks::{
    parse_retention_arg, run_all_configured, run_task, MaintenanceConfig, TaskContext, TaskId,
};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;

pub async fn tasks(args: &Args, cmd: &TasksCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    ctx.require_db()?;
    let pool = ctx.open_pool().await?;
    let mailbox = MailboxStore::new(&ctx.state_dir);
    let maintenance = MaintenanceConfig::from_runtime(&pool, &ctx.config).await?;
    let task_ctx = TaskContext {
        pool: &pool,
        mailbox: &mailbox,
        maintenance: &maintenance,
    };

    match cmd {
        TasksCommand::List => {
            println!("Maintenance jobs (also run on a schedule when `chatmail run` is active):\n");
            for id in TaskId::ALL {
                let enabled = match id {
                    TaskId::PruneOldMessages => maintenance.message_retention.is_some(),
                    TaskId::PruneUnusedAccounts => maintenance.unused_account_retention.is_some(),
                    TaskId::PurgeSeenMessages => {
                        println!(
                            "  {} — {} (in-process every 15s when __AUTO_PURGE_SEEN__=enabled)",
                            id.name(),
                            id.description()
                        );
                        continue;
                    }
                    TaskId::PruneUnreadOlder => false,
                };
                let cfg_note = if enabled {
                    " [enabled — DB or maddy.conf]"
                } else {
                    ""
                };
                println!("  {} — {}{}", id.name(), id.description(), cfg_note);
            }
            println!();
            if let Some(r) = maintenance.message_retention {
                println!("message file retention: {:?}", r);
            }
            if let Some(r) = maintenance.unused_account_retention {
                println!("storage.imapsql unused_account_retention: {:?}", r);
            }
            if !maintenance.periodic_jobs_enabled() {
                println!(
                    "No periodic jobs — enable message retention in admin UI or set retention / unused_account_retention in maddy.conf"
                );
            } else {
                println!("Periodic interval when server is running: 1h (Madmail parity)");
            }
        }
        TasksCommand::Run { task, retention } => {
            let id = TaskId::parse(task).ok_or_else(|| {
                ChatmailError::config(format!(
                    "unknown task {task:?}; use `chatmail tasks list`"
                ))
            })?;
            let retention_override = match retention {
                Some(s) => Some(parse_retention_arg(s)?),
                None => None,
            };
            let outcome = run_task(&task_ctx, id, retention_override).await?;
            if outcome.skipped {
                println!(
                    "{}: skipped ({})",
                    id.name(),
                    outcome.detail.unwrap_or_default()
                );
            } else {
                println!(
                    "{}: deleted {} item(s){}",
                    id.name(),
                    outcome.deleted,
                    outcome
                        .detail
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default()
                );
            }
        }
        TasksCommand::RunAll => {
            let report = run_all_configured(&task_ctx).await?;
            if report.outcomes.is_empty() {
                println!("No jobs enabled in config (set storage.imapsql retention directives).");
            }
            for outcome in report.outcomes {
                if outcome.skipped {
                    println!(
                        "{}: skipped ({})",
                        outcome.task.name(),
                        outcome.detail.unwrap_or_default()
                    );
                } else {
                    println!(
                        "{}: deleted {} item(s)",
                        outcome.task.name(),
                        outcome.deleted
                    );
                }
            }
        }
    }
    Ok(())
}
