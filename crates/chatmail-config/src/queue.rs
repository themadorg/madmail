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

//! `target.queue` settings (Madmail `internal/target/queue/queue.go`).

use std::path::PathBuf;

/// Parsed from `target.queue remote_queue { ... }` in `maddy.conf`.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueSettings {
    /// Queue directory (default: `{state_dir}/remote_queue`).
    pub location: Option<PathBuf>,
    /// Maximum delivery attempts per recipient (default: 3).
    pub max_tries: u32,
    /// Concurrent deliveries (default: 16).
    pub max_parallelism: u32,
    /// First retry delay in seconds (default: 60 = 1m).
    pub initial_retry_secs: u64,
    /// Exponential backoff factor (default: 1.25).
    pub retry_time_scale: f64,
    /// Delay before processing queue entries loaded at startup (default: 10s).
    pub post_init_delay_secs: u64,
    /// Max time a message may stay in the outbound queue (default: 600s = 10m).
    /// After this, the message is dropped as failed (madmail-v2; Madmail retries much longer).
    pub max_delivery_secs: u64,
}

impl Default for QueueSettings {
    fn default() -> Self {
        Self {
            location: None,
            max_tries: 3,
            max_parallelism: 16,
            initial_retry_secs: 60,
            retry_time_scale: 1.25,
            post_init_delay_secs: 10,
            max_delivery_secs: 10 * 60,
        }
    }
}

impl QueueSettings {
    pub fn effective_location(&self, state_dir: &std::path::Path) -> PathBuf {
        self.location
            .clone()
            .unwrap_or_else(|| state_dir.join("remote_queue"))
    }
}
