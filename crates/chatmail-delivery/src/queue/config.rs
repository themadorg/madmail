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

use std::path::{Path, PathBuf};
use std::time::Duration;

use chatmail_config::QueueSettings;

/// Runtime queue configuration (Madmail `target.queue`).
#[derive(Debug, Clone)]
pub struct QueueConfig {
    pub location: PathBuf,
    pub max_tries: u32,
    pub max_parallelism: usize,
    pub initial_retry: Duration,
    pub retry_time_scale: f64,
    pub post_init_delay: Duration,
    /// Drop queued messages older than this (failed permanently).
    pub max_delivery_time: Duration,
}

impl QueueConfig {
    pub fn from_settings(state_dir: &Path, settings: &QueueSettings) -> Self {
        Self {
            location: settings.effective_location(state_dir),
            max_tries: settings.max_tries.max(1),
            max_parallelism: settings.max_parallelism.max(1) as usize,
            initial_retry: Duration::from_secs(settings.initial_retry_secs.max(1)),
            retry_time_scale: settings.retry_time_scale.max(1.0),
            post_init_delay: Duration::from_secs(settings.post_init_delay_secs),
            max_delivery_time: Duration::from_secs(settings.max_delivery_secs.max(1)),
        }
    }

    /// True when the message has been in the queue longer than `max_delivery_time`.
    pub fn is_expired(&self, meta: &super::store::QueueMeta) -> bool {
        let queued_at = meta.effective_queued_at();
        if queued_at == 0 {
            return false;
        }
        let age = super::store::now_unix().saturating_sub(queued_at);
        age >= self.max_delivery_time.as_secs()
    }

    /// Delay before attempt `tries_count` (1-based after a failure).
    pub fn retry_delay(&self, tries_count: u32) -> Duration {
        let exp = tries_count.saturating_sub(1) as i32;
        let scale = self.retry_time_scale.powi(exp);
        let secs = (self.initial_retry.as_secs_f64() * scale).round() as u64;
        Duration::from_secs(secs.max(1))
    }
}
