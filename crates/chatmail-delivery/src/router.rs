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
        let mut remote_enqueued = 0usize;

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
            self.enqueue_remote(mail_from.to_string(), rcpt, data.to_vec())
                .await?;
            remote_enqueued += 1;
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
                    self.enqueue_remote(mail_from.to_string(), rcpt, data.to_vec())
                        .await?;
                }
            }
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
