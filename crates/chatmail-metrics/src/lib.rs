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

//! Prometheus / OpenMetrics exporter (Madmail `openmetrics` endpoint).

mod metrics;
mod server;

pub use metrics::{
    exposition_text, init_metrics, record_smtp_aborted, record_smtp_completed,
    record_smtp_failed_command, record_smtp_failed_login, record_smtp_started,
    set_queue_length,
};
pub use server::run_openmetrics_listener;
