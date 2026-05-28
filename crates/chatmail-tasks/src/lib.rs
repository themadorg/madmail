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

//! Scheduled maintenance jobs (Madmail `imapsql` cleanup loops + auto-purge seen).

mod config;
mod jobs;
mod scheduler;

pub use config::MaintenanceConfig;
pub use jobs::{
    parse_retention_arg, run_all_configured, run_task, TaskContext, TaskId, TaskOutcome,
    TaskRunReport,
};
pub use scheduler::{spawn_maintenance_scheduler, MaintenanceHandle};
