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
mod unit {
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

    #[test]
    fn from_settings_clamps_invalid_values_to_minimums() {
        let cfg = QueueConfig::from_settings(
            std::path::Path::new("/state"),
            &QueueSettings {
                max_tries: 0,
                max_parallelism: 0,
                initial_retry_secs: 0,
                retry_time_scale: 0.5,
                max_delivery_secs: 0,
                ..QueueSettings::default()
            },
        );
        assert_eq!(cfg.max_tries, 1);
        assert_eq!(cfg.max_parallelism, 1);
        assert_eq!(cfg.initial_retry, Duration::from_secs(1));
        assert!((cfg.retry_time_scale - 1.0).abs() < f64::EPSILON);
        assert_eq!(cfg.max_delivery_time, Duration::from_secs(1));
        assert_eq!(
            cfg.location,
            std::path::Path::new("/state").join("remote_queue")
        );
    }

    #[test]
    fn is_expired_falls_back_to_last_attempt_for_legacy_meta() {
        use super::super::store::{now_unix, QueueMeta};

        let cfg = QueueConfig::from_settings(
            std::path::Path::new("/tmp"),
            &QueueSettings {
                max_delivery_secs: 60,
                ..QueueSettings::default()
            },
        );
        let now = now_unix();
        let legacy = QueueMeta {
            id: "legacy".into(),
            mail_from: "a@b".into(),
            rcpt_to: "c@d".into(),
            tries_count: 1,
            queued_at_unix: 0,
            last_attempt_unix: now.saturating_sub(120),
            next_attempt_unix: now,
            last_error: None,
        };
        assert!(cfg.is_expired(&legacy));
    }

    #[test]
    fn effective_queued_at_prefers_queued_at_unix() {
        use super::super::store::QueueMeta;

        let meta = QueueMeta {
            id: "x".into(),
            mail_from: "a@b".into(),
            rcpt_to: "c@d".into(),
            tries_count: 0,
            queued_at_unix: 100,
            last_attempt_unix: 200,
            next_attempt_unix: 300,
            last_error: None,
        };
        assert_eq!(meta.effective_queued_at(), 100);
    }
}

/// End-to-end queue wiring: a failed entry ends in one report in the sender's inbox.
#[cfg(test)]
mod wiring {
    use std::sync::Arc;
    use std::time::Duration;

    use chatmail_state::{AppState, TransportPolicy, ALL_BLOCKED_REASON};

    use super::super::store::now_unix;
    use super::super::{OutboundQueue, QueueConfig, QueueStore};
    use crate::router::{DeliveryContext, OutboundJob};

    const SENDER: &str = "a@example.org";
    const ORIGINAL: &[u8] =
        b"From: a@example.org\r\nTo: b@other.org\r\nMessage-ID: <q@example.org>\r\n\r\nbody";
    const ALL_BLOCKED: TransportPolicy = TransportPolicy {
        https: false,
        http: false,
        smtp: false,
    };

    async fn setup(
        policy: TransportPolicy,
        max_tries: u32,
    ) -> (
        DeliveryContext,
        Arc<AppState>,
        QueueConfig,
        tempfile::TempDir,
    ) {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path(), pool.clone()));
        state.federation_transport.set(policy);
        state.auth.insert(SENDER, "bcrypt:test");
        let ctx = DeliveryContext {
            pool,
            state: Arc::clone(&state),
            primary_domain: "example.org".into(),
            local_domains: vec!["example.org".into()],
            dkim: None,
        };
        let config = QueueConfig {
            location: dir.path().join("remote_queue"),
            max_tries,
            max_parallelism: 4,
            initial_retry: Duration::from_secs(60),
            retry_time_scale: 1.0,
            post_init_delay: Duration::ZERO,
            max_delivery_time: Duration::from_secs(600),
        };
        (ctx, state, config, dir)
    }

    /// Wait for exactly one report in the sender's `INBOX/new` and return it.
    async fn wait_for_report(state: &AppState) -> String {
        let new_dir = state.mailbox_store.maildir_for_user(SENDER).new;
        for _ in 0..200 {
            if let Ok(entries) = std::fs::read_dir(&new_dir) {
                let files: Vec<_> = entries.filter_map(|e| e.ok()).collect();
                if !files.is_empty() {
                    assert_eq!(files.len(), 1, "one report per failed entry");
                    let raw = std::fs::read(files[0].path()).unwrap();
                    return String::from_utf8(raw).unwrap();
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("no delivery report in {}", new_dir.display());
    }

    async fn wait_for_empty_queue(queue: &OutboundQueue) {
        for _ in 0..200 {
            if queue.depth().await.unwrap() == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("queue entry was not removed");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn transport_policy_block_reports_a_permanent_failure() {
        let (ctx, state, config, _dir) = setup(ALL_BLOCKED, 3).await;
        let queue = OutboundQueue::start(ctx, config).await.unwrap();
        queue
            .enqueue(OutboundJob {
                mail_from: SENDER.into(),
                rcpt_to: "b@other.org".into(),
                data: ORIGINAL.to_vec(),
            })
            .await
            .unwrap();
        let report = wait_for_report(&state).await;
        assert!(report.contains("report-type=delivery-status"), "{report}");
        assert!(report.contains("Status: 5.7.1"), "{report}");
        assert!(report.contains("was not delivered"), "{report}");
        assert!(report.contains(ALL_BLOCKED_REASON), "{report}");
        assert!(report.contains("recipient domain other.org"), "{report}");
        assert!(report.contains("Message-ID: <q@example.org>"), "{report}");
        wait_for_empty_queue(&queue).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn retries_used_up_report_gave_up() {
        let policy = TransportPolicy {
            https: true,
            http: false,
            smtp: false,
        };
        let (ctx, state, config, _dir) = setup(policy, 1).await;
        // Nothing listens on port 1: a transient HTTPS error, and SMTP is blocked.
        chatmail_db::set_endpoint_override(
            &ctx.pool,
            "other.org",
            "https://127.0.0.1:1/mxdeliv",
            "test",
        )
        .await
        .unwrap();
        let queue = OutboundQueue::start(ctx, config).await.unwrap();
        queue
            .enqueue(OutboundJob {
                mail_from: SENDER.into(),
                rcpt_to: "b@other.org".into(),
                data: ORIGINAL.to_vec(),
            })
            .await
            .unwrap();
        let report = wait_for_report(&state).await;
        assert!(report.contains("after repeated attempts"), "{report}");
        assert!(report.contains("Status: 5.4.7"), "{report}");
        assert!(
            report.contains("smtp blocked by outbound transport policy"),
            "{report}"
        );
        wait_for_empty_queue(&queue).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn entry_expired_on_reload_reports_its_last_error() {
        let (ctx, state, config, _dir) = setup(ALL_BLOCKED, 3).await;
        let store = QueueStore::new(config.location.clone());
        store.ensure_dir().await.unwrap();
        store
            .write_new("old", SENDER, "b@other.org", ORIGINAL, now_unix())
            .await
            .unwrap();
        let mut meta = store.read_meta("old").await.unwrap();
        meta.queued_at_unix = now_unix().saturating_sub(3600);
        meta.tries_count = 1;
        meta.last_error = Some("peer unreachable: connection refused".into());
        store.update_meta(&meta).await.unwrap();

        let queue = OutboundQueue::start(ctx, config).await.unwrap();
        let report = wait_for_report(&state).await;
        assert!(
            report.contains("expired before it could be delivered"),
            "{report}"
        );
        assert!(report.contains("Status: 5.4.7"), "{report}");
        assert!(
            report.contains("peer unreachable: connection refused"),
            "{report}"
        );
        assert!(report.contains("Message-ID: <q@example.org>"), "{report}");
        wait_for_empty_queue(&queue).await;
    }
}
