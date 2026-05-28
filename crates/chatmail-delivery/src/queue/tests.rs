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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chatmail_config::QueueSettings;

    use super::super::config::QueueConfig;

    #[test]
    fn retry_delay_exponential() {
        let cfg = QueueConfig::from_settings(
            std::path::Path::new("/tmp"),
            &QueueSettings {
                initial_retry_secs: 900,
                retry_time_scale: 1.25,
                ..QueueSettings::default()
            },
        );
        assert_eq!(cfg.retry_delay(1), Duration::from_secs(900));
        assert_eq!(cfg.retry_delay(2), Duration::from_secs(1125));
    }

    #[test]
    fn max_delivery_time_default_ten_minutes() {
        assert_eq!(QueueSettings::default().max_delivery_secs, 600);
    }

    #[test]
    fn is_expired_after_max_delivery_time() {
        use super::super::store::{now_unix, QueueMeta};

        let cfg = QueueConfig::from_settings(
            std::path::Path::new("/tmp"),
            &QueueSettings {
                max_delivery_secs: 600,
                ..QueueSettings::default()
            },
        );
        let now = now_unix();
        let fresh = QueueMeta {
            id: "a".into(),
            mail_from: "b@c".into(),
            rcpt_to: "d@e".into(),
            tries_count: 0,
            queued_at_unix: now,
            last_attempt_unix: 0,
            next_attempt_unix: now,
            last_error: None,
        };
        assert!(!cfg.is_expired(&fresh));

        let stale = QueueMeta {
            queued_at_unix: now.saturating_sub(601),
            ..fresh.clone()
        };
        assert!(cfg.is_expired(&stale));
    }
}
