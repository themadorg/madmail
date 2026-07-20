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

use std::collections::HashMap;
use std::sync::Arc;

use chatmail_auth::normalize_username;
use chatmail_config::QueueSettings;
use chatmail_db::DbPool;
use chatmail_state::AppState;
use chatmail_types::{address_domain, address_is_local, ChatmailError, Result};
use tokio::sync::OnceCell;
use tracing::{debug, info, warn};

use crate::queue::{OutboundQueue, QueueConfig};

#[derive(Debug, Clone)]
pub struct OutboundJob {
    pub mail_from: String,
    pub rcpt_to: String,
    pub data: Vec<u8>,
}

pub struct DeliveryContext {
    pub pool: DbPool,
    pub state: Arc<AppState>,
    pub primary_domain: String,
    /// All domains accepted for local delivery (`$(local_domains)` + forms).
    pub local_domains: Vec<String>,
}

static OUTBOUND_QUEUE: OnceCell<Arc<OutboundQueue>> = OnceCell::const_new();

/// Start disk-backed outbound queue + worker (Madmail `target.queue remote_queue`).
pub async fn start_outbound_queue(
    ctx: DeliveryContext,
    state_dir: &std::path::Path,
    queue_settings: &QueueSettings,
) -> Result<Arc<OutboundQueue>> {
    let config = QueueConfig::from_settings(state_dir, queue_settings);
    let queue = OutboundQueue::start(ctx, config).await?;
    let _ = OUTBOUND_QUEUE.set(Arc::clone(&queue));
    Ok(queue)
}

pub fn outbound_queue() -> Option<Arc<OutboundQueue>> {
    OUTBOUND_QUEUE.get().cloned()
}

impl DeliveryContext {
    pub fn is_local(&self, rcpt: &str) -> bool {
        address_is_local(rcpt, &self.local_domains)
    }

    pub async fn enqueue_remote(
        &self,
        mail_from: String,
        rcpt_to: String,
        data: Vec<u8>,
    ) -> Result<()> {
        let job = OutboundJob {
            mail_from,
            rcpt_to,
            data,
        };
        let queue = OUTBOUND_QUEUE
            .get()
            .ok_or_else(|| ChatmailError::storage("outbound queue not started"))?;
        queue.enqueue(job).await
    }

    /// Enqueue a message for many remote recipients at once (federated group fan-out),
    /// writing the body once and hard-linking it into each recipient's queue entry.
    pub async fn enqueue_remote_batch(
        &self,
        mail_from: &str,
        rcpts: &[String],
        data: &[u8],
    ) -> Result<()> {
        let queue = OUTBOUND_QUEUE
            .get()
            .ok_or_else(|| ChatmailError::storage("outbound queue not started"))?;
        queue.enqueue_batch(mail_from, rcpts, data).await
    }

    /// Authenticated mail submission — **shared** by SMTP AUTH (587/465) and WebSMTP.
    ///
    /// Security (caller must still run `validate_submission_headers` + `enforce_encryption`):
    /// - recipients normalized
    /// - per-recipient quota
    /// - local → maildir; remote → same outbound federation queue as SMTP
    /// - federation policy + silent-dismiss on remote RCPT
    ///
    /// Does **not** treat the message as inbound federation (no inbound metrics / policy bypass).
    pub async fn submit_authenticated(
        &self,
        mail_from: &str,
        recipients: &[String],
        data: &[u8],
    ) -> Result<()> {
        self.state.check_message_size(data.len())?;

        let ingest_start = std::time::Instant::now();
        let total_rcpts = recipients.len();
        let mut local_deliveries: Vec<(String, String)> = Vec::new();
        let mut remote_rcpts: Vec<String> = Vec::new();

        for raw_rcpt in recipients {
            let rcpt = normalize_username(raw_rcpt)?;
            self.state.quota.check_quota(&rcpt, data.len() as u64)?;

            if self.is_local(&rcpt) {
                // Authenticated submission may deliver to any local address (SMTP AUTH parity).
                local_deliveries.push((rcpt, uuid::Uuid::new_v4().to_string()));
                continue;
            }

            let domain = address_domain(&rcpt).unwrap_or_default();
            let mode = self.state.federation_policy.global_mode();
            if !domain.is_empty() && !self.state.federation_policy.check_policy(&domain, mode) {
                return Err(ChatmailError::FederationRejected(domain));
            }
            if self
                .state
                .federation_silent_dismiss
                .is_dismissed(&rcpt, &self.local_domains)
            {
                debug!(rcpt = %rcpt, "silently dismissed outbound federation message");
                continue;
            }
            remote_rcpts.push(rcpt);
        }

        // Federated group fan-out: one body write + hard-links (same principle as local
        // delivery) rather than a full separate copy per remote recipient.
        let remote_enqueued = remote_rcpts.len();
        if !remote_rcpts.is_empty() {
            self.enqueue_remote_batch(mail_from, &remote_rcpts, data)
                .await?;
        }

        let rcpt_phase = ingest_start.elapsed();
        if !local_deliveries.is_empty() {
            let local_n = local_deliveries.len();
            let deliver_start = std::time::Instant::now();
            let outcome = chatmail_storage::deliver_local_messages(
                &self.state.mailbox_store,
                &local_deliveries,
                data,
            )
            .await?;
            let deliver_ms = deliver_start.elapsed();
            let notify_start = std::time::Instant::now();
            for (rcpt, msg_id) in &outcome.delivered {
                self.state.quota.record_write(rcpt, data.len() as u64);
                self.state.events.notify_new_message(rcpt, msg_id);
                self.state
                    .notify_inbound_push(&self.pool, mail_from, rcpt)
                    .await;
            }
            if !outcome.failed.is_empty() {
                for (rcpt, _msg_id, err) in &outcome.failed {
                    warn!(rcpt = %rcpt, error = %err, "local delivery failed for recipient");
                }
                warn!(
                    delivered = outcome.delivered.len(),
                    failed = outcome.failed.len(),
                    "partial local fan-out: some recipients did not receive the message"
                );
            }
            info!(
                total_rcpts,
                local_n,
                remote_enqueued,
                delivered = outcome.delivered.len(),
                failed = outcome.failed.len(),
                rcpt_phase_ms = rcpt_phase.as_millis(),
                deliver_ms = deliver_ms.as_millis(),
                notify_ms = notify_start.elapsed().as_millis(),
                ingest_ms = ingest_start.elapsed().as_millis(),
                "authenticated submission fan-out timing"
            );
        } else if remote_enqueued > 0 {
            info!(
                total_rcpts,
                remote_enqueued,
                rcpt_phase_ms = rcpt_phase.as_millis(),
                "authenticated submission enqueued for federation"
            );
        }
        Ok(())
    }

    /// Inbound / mixed routing (unauthenticated paths, tests). Prefer
    /// [`Self::submit_authenticated`] for SMTP AUTH and WebSMTP.
    pub async fn route_message(
        &self,
        mail_from: &str,
        recipients: &[String],
        data: &[u8],
    ) -> Result<()> {
        self.state.check_message_size(data.len())?;
        let mut by_domain: HashMap<String, Vec<String>> = HashMap::new();
        for r in recipients {
            if let Some(d) = rcpt_domain(r) {
                by_domain.entry(d).or_default().push(r.clone());
            }
        }
        if chatmail_db::is_federation_sender_blocked(mail_from) {
            debug!(from = %mail_from, "silently dropped inbound from blocked sender");
            return Ok(());
        }

        let mut local_deliveries: Vec<(String, String)> = Vec::new();
        let mut remote_rcpts: Vec<String> = Vec::new();

        for (domain, rcpts) in by_domain {
            if self.local_domains.iter().any(|d| {
                chatmail_types::domain_forms(d)
                    .iter()
                    .any(|f| f.eq_ignore_ascii_case(&domain))
            }) {
                for rcpt in rcpts {
                    if !self.state.auth.local_recipient_allowed(&rcpt) {
                        debug!(rcpt = %rcpt, "silently dropped inbound local delivery");
                        continue;
                    }
                    self.state.quota.check_quota(&rcpt, data.len() as u64)?;
                    local_deliveries.push((rcpt, uuid::Uuid::new_v4().to_string()));
                }
            } else {
                let mode = self.state.federation_policy.global_mode();
                for rcpt in rcpts {
                    if !self.state.federation_policy.check_policy(&domain, mode) {
                        return Err(ChatmailError::FederationRejected(domain.clone()));
                    }
                    if self
                        .state
                        .federation_silent_dismiss
                        .is_dismissed(&rcpt, &self.local_domains)
                    {
                        debug!(rcpt = %rcpt, "silently dismissed outbound federation message");
                        continue;
                    }
                    remote_rcpts.push(rcpt);
                }
            }
        }

        // Federated group fan-out: one body write + hard-links (same principle as local
        // delivery) rather than a full separate copy per remote recipient.
        if !remote_rcpts.is_empty() {
            self.enqueue_remote_batch(mail_from, &remote_rcpts, data)
                .await?;
        }

        if !local_deliveries.is_empty() {
            let outcome = chatmail_storage::deliver_local_messages(
                &self.state.mailbox_store,
                &local_deliveries,
                data,
            )
            .await?;
            for (rcpt, msg_id) in &outcome.delivered {
                self.state.quota.record_write(rcpt, data.len() as u64);
                self.state.events.notify_new_message(rcpt, msg_id);
                self.state
                    .notify_inbound_push(&self.pool, mail_from, rcpt)
                    .await;
            }
            if !outcome.failed.is_empty() {
                for (rcpt, _msg_id, err) in &outcome.failed {
                    warn!(rcpt = %rcpt, error = %err, "local delivery failed for recipient");
                }
                warn!(
                    delivered = outcome.delivered.len(),
                    failed = outcome.failed.len(),
                    "partial local fan-out: some recipients did not receive the message"
                );
            }
        }
        chatmail_db::record_inbound_delivery();
        Ok(())
    }
}

fn rcpt_domain(addr: &str) -> Option<String> {
    address_domain(addr)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use chatmail_config::QueueSettings;
    use chatmail_db::init_memory_db;
    use chatmail_state::AppState;

    /// P8-UT01: local vs remote routing by domain.
    #[test]
    fn p8_ut01_test_router_local_vs_remote() {
        let local = chatmail_types::build_local_domains("example.org", None);
        assert!(address_is_local("u@example.org", &local));
        assert!(!address_is_local("u@other.org", &local));
    }

    #[test]
    fn p8_ut01_test_router_ip_and_domain_install() {
        let local = chatmail_types::build_local_domains("a.com", Some("a.com [1.1.1.1]"));
        assert!(address_is_local("u@a.com", &local));
        assert!(address_is_local("u@[1.1.1.1]", &local));
        assert!(address_is_local("u@1.1.1.1", &local));
    }

    #[tokio::test]
    async fn silent_dismiss_skips_remote_enqueue() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.auth.hydrate(&pool).await.unwrap();
        app.federation_silent_dismiss
            .add(&pool, "remote.test")
            .await
            .unwrap();
        let local_domains = chatmail_types::build_local_domains("local.test", None);
        let ctx = DeliveryContext {
            pool: pool.clone(),
            state: Arc::clone(&app),
            primary_domain: "local.test".into(),
            local_domains: local_domains.clone(),
        };
        start_outbound_queue(
            DeliveryContext {
                pool: pool.clone(),
                state: Arc::clone(&app),
                primary_domain: "local.test".into(),
                local_domains: local_domains.clone(),
            },
            dir.path(),
            &QueueSettings::default(),
        )
        .await
        .unwrap();

        let body = b"From: a@local.test\r\nTo: b@remote.test\r\n\r\nx";
        ctx.route_message("a@local.test", &["b@remote.test".into()], body)
            .await
            .unwrap();

        let queue_dir = dir.path().join("remote_queue");
        let store = crate::queue::QueueStore::new(queue_dir);
        assert_eq!(store.count_entries().await.unwrap(), 0);
    }

    /// Federated group fan-out writes a durable queue entry for every remote
    /// recipient (shared-body + hard-link batch path). Uses the returned queue
    /// handle directly to stay isolated from the process-wide `OUTBOUND_QUEUE`
    /// other tests may set.
    #[tokio::test]
    async fn enqueue_remote_batch_writes_every_recipient() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.auth.hydrate(&pool).await.unwrap();
        let local_domains = chatmail_types::build_local_domains("local.test", None);
        let queue = start_outbound_queue(
            DeliveryContext {
                pool: pool.clone(),
                state: Arc::clone(&app),
                primary_domain: "local.test".into(),
                local_domains,
            },
            dir.path(),
            &QueueSettings::default(),
        )
        .await
        .unwrap();

        let recipients: Vec<String> = (0..25).map(|i| format!("u{i}@remote.test")).collect();
        let body = b"From: a@local.test\r\nTo: g@remote.test\r\n\r\nx";
        queue
            .enqueue_batch("a@local.test", &recipients, body)
            .await
            .unwrap();

        // Every remote recipient got its own durable queue entry. Delivery to the
        // bogus domain fails transiently and requeues (max_tries=3), so all entries
        // remain on disk for this assertion.
        let queue_dir = dir.path().join("remote_queue");
        let store = crate::queue::QueueStore::new(queue_dir.clone());
        assert_eq!(store.count_entries().await.unwrap(), recipients.len());

        // madmail single-write + hard-link principle: the body bytes exist once on
        // disk (one inode) and every recipient's `.body` is a hard link to it.
        use std::os::unix::fs::MetadataExt;
        let mut bodies = std::fs::read_dir(&queue_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "body"))
            .collect::<Vec<_>>();
        assert_eq!(bodies.len(), recipients.len());
        let inodes: std::collections::HashSet<u64> = bodies
            .iter_mut()
            .map(|p| std::fs::metadata(&*p).unwrap().ino())
            .collect();
        assert_eq!(inodes.len(), 1, "all bodies must share a single inode");
        let nlink = std::fs::metadata(&bodies[0]).unwrap().nlink();
        assert_eq!(nlink as usize, recipients.len());
    }

    #[tokio::test]
    async fn enqueue_batch_empty_is_ok() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.auth.hydrate(&pool).await.unwrap();
        let local_domains = chatmail_types::build_local_domains("local.test", None);
        let queue = start_outbound_queue(
            DeliveryContext {
                pool: pool.clone(),
                state: Arc::clone(&app),
                primary_domain: "local.test".into(),
                local_domains,
            },
            dir.path(),
            &QueueSettings::default(),
        )
        .await
        .unwrap();

        queue
            .enqueue_batch("a@local.test", &[], b"nobody")
            .await
            .unwrap();
        assert_eq!(queue.depth().await.unwrap(), 0);
        let store = crate::queue::QueueStore::new(dir.path().join("remote_queue"));
        assert_eq!(store.count_entries().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn enqueue_batch_single_recipient_writes_meta_and_body() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.auth.hydrate(&pool).await.unwrap();
        let local_domains = chatmail_types::build_local_domains("local.test", None);
        let queue = start_outbound_queue(
            DeliveryContext {
                pool: pool.clone(),
                state: Arc::clone(&app),
                primary_domain: "local.test".into(),
                local_domains,
            },
            dir.path(),
            &QueueSettings::default(),
        )
        .await
        .unwrap();

        let body = b"From: a@local.test\r\nTo: u@remote.test\r\n\r\none";
        queue
            .enqueue_batch("a@local.test", &["u@remote.test".into()], body)
            .await
            .unwrap();

        // Transient delivery failure requeues; entry stays for inspection.
        let store = crate::queue::QueueStore::new(dir.path().join("remote_queue"));
        assert_eq!(store.count_entries().await.unwrap(), 1);
        let id = store.list_ids().await.unwrap().pop().unwrap();
        let (meta, loaded) = store.load(&id).await.unwrap();
        assert_eq!(meta.mail_from, "a@local.test");
        assert_eq!(meta.rcpt_to, "u@remote.test");
        assert_eq!(loaded, body);
    }

    #[tokio::test]
    async fn enqueue_batch_rolls_back_on_write_failure() {
        use crate::queue::test_failpoints;

        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.auth.hydrate(&pool).await.unwrap();
        let local_domains = chatmail_types::build_local_domains("local.test", None);
        let queue = start_outbound_queue(
            DeliveryContext {
                pool: pool.clone(),
                state: Arc::clone(&app),
                primary_domain: "local.test".into(),
                local_domains,
            },
            dir.path(),
            &QueueSettings::default(),
        )
        .await
        .unwrap();

        let _fp = test_failpoints::arm(test_failpoints::AFTER_FIRST_META);
        let err = queue
            .enqueue_batch(
                "a@local.test",
                &[
                    "u0@remote.test".into(),
                    "u1@remote.test".into(),
                    "u2@remote.test".into(),
                ],
                b"will-fail",
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("test failpoint"));

        // Failed batch must not leave durable entries (worker must not see them).
        let store = crate::queue::QueueStore::new(dir.path().join("remote_queue"));
        assert_eq!(store.count_entries().await.unwrap(), 0);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("remote_queue"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            leftovers.is_empty(),
            "enqueue_batch rollback left files: {leftovers:?}"
        );
    }

    /// Local group delivery uses in-memory auth cache (no per-recipient DB lookups).
    #[tokio::test]
    async fn route_message_local_group_uses_auth_cache() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::passwords::create_user(&pool, "u1@local.test", "bcrypt:x")
            .await
            .unwrap();
        chatmail_db::passwords::create_user(&pool, "u2@local.test", "bcrypt:x")
            .await
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.auth.hydrate(&pool).await.unwrap();

        let local_domains = chatmail_types::build_local_domains("local.test", None);
        let ctx = DeliveryContext {
            pool: pool.clone(),
            state: Arc::clone(&app),
            primary_domain: "local.test".into(),
            local_domains,
        };

        let recipients: Vec<String> = (1..=60)
            .map(|i| format!("u{}@local.test", (i % 2) + 1))
            .collect();
        let body = b"From: a@local.test\r\nTo: u1@local.test\r\n\r\nhello";
        ctx.route_message("a@local.test", &recipients, body)
            .await
            .unwrap();

        assert_eq!(app.auth.len(), 2);
    }
}
