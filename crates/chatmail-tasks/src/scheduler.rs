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

//! In-process maintenance scheduler (started with `chatmail run`).

use std::path::Path;
use chatmail_config::AppConfig;
use chatmail_db::DbPool;
use chatmail_storage::MailboxStore;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

use crate::config::{MaintenanceConfig, AUTO_PURGE_SEEN_INTERVAL, PERIODIC_INTERVAL};
use crate::jobs::{run_all_configured, run_auto_purge_seen_if_enabled, TaskContext};

pub struct MaintenanceHandle {
    cancel: CancellationToken,
    join: JoinHandle<()>,
}

impl MaintenanceHandle {
    pub fn shutdown(self) {
        self.cancel.cancel();
    }

    pub async fn wait(self) {
        let _ = self.join.await;
    }
}

/// Background loops: hourly retention/unused-account jobs + 15s auto-purge seen (when enabled in DB).
pub fn spawn_maintenance_scheduler(
    pool: DbPool,
    state_dir: &Path,
    file_config: &AppConfig,
) -> MaintenanceHandle {
    let cancel = CancellationToken::new();
    let cancel_child = cancel.clone();
    let state_dir = state_dir.to_path_buf();
    let file_config = file_config.clone();

    let join = tokio::spawn(async move {
        let mailbox = MailboxStore::new(&state_dir);
        let maintenance = match MaintenanceConfig::from_runtime(&pool, &file_config).await {
            Ok(m) => m,
            Err(e) => {
                error!("maintenance: invalid config: {e}");
                return;
            }
        };

        let mut periodic_tick = tokio::time::interval(PERIODIC_INTERVAL);
        periodic_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut seen_tick = tokio::time::interval(AUTO_PURGE_SEEN_INTERVAL);
        seen_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // First periodic run after one interval (Madmail starts ticker then waits).
        periodic_tick.tick().await;
        seen_tick.tick().await;

        loop {
            tokio::select! {
                _ = cancel_child.cancelled() => break,
                _ = periodic_tick.tick() => {
                    let ctx = TaskContext {
                        pool: &pool,
                        mailbox: &mailbox,
                        maintenance: &maintenance,
                    };
                    match run_all_configured(&ctx).await {
                        Ok(report) => {
                            for o in report.outcomes {
                                if o.skipped {
                                    debug!(task = o.task.name(), "maintenance: skipped");
                                } else {
                                    debug!(
                                        task = o.task.name(),
                                        deleted = o.deleted,
                                        detail = ?o.detail,
                                        "maintenance: completed"
                                    );
                                }
                            }
                        }
                        Err(e) => error!("maintenance periodic run failed: {e}"),
                    }
                }
                _ = seen_tick.tick() => {
                    match run_auto_purge_seen_if_enabled(&pool, &mailbox).await {
                        Ok(Some(n)) if n > 0 => {
                            debug!(deleted = n, "auto-purge seen messages");
                        }
                        Ok(_) => {}
                        Err(e) => error!("auto-purge seen failed: {e}"),
                    }
                }
            }
        }
    });

    MaintenanceHandle { cancel, join }
}
