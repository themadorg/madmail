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

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chatmail_types::Result;
use tokio::sync::{mpsc, Semaphore};
use tracing::{info, warn};

use crate::router::{DeliveryContext, OutboundJob};
use crate::transport::{deliver_remote, DeliveryOutcome};

use super::config::QueueConfig;
use super::store::{now_unix, QueueStore};

pub struct OutboundQueue {
    ctx: DeliveryContext,
    config: QueueConfig,
    store: QueueStore,
    work_tx: mpsc::UnboundedSender<String>,
    started_at: SystemTime,
}

impl OutboundQueue {
    pub async fn start(ctx: DeliveryContext, config: QueueConfig) -> Result<Arc<Self>> {
        let store = QueueStore::new(config.location.clone());
        store.ensure_dir().await?;

        let (work_tx, work_rx) = mpsc::unbounded_channel();
        let max_delivery = config.max_delivery_time;
        let queue = Arc::new(Self {
            ctx,
            config,
            store,
            work_tx,
            started_at: SystemTime::now(),
        });

        let runner = Arc::clone(&queue);
        tokio::spawn(runner.run_worker(work_rx));

        queue.reload_disk_queue().await?;

        info!(
            path = %queue.store.location().display(),
            ?max_delivery,
            max_tries = queue.config.max_tries,
            "outbound retry queue configured"
        );

        Ok(queue)
    }

    pub async fn enqueue(&self, job: OutboundJob) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        self.store
            .write_new(&id, &job.mail_from, &job.rcpt_to, &job.data, now_unix())
            .await?;
        let _ = self.work_tx.send(id);
        Ok(())
    }

    /// Enqueue one message for many remote recipients — the federated group
    /// fan-out path.
    ///
    /// Applies the same madmail principle as local group delivery: the body is
    /// written to disk **once** and hard-linked into every other recipient's queue
    /// entry (same bytes, one inode, no second copy), instead of writing a full
    /// separate copy per recipient. Each entry is dispatched to the worker only
    /// after its body and meta are durable.
    pub async fn enqueue_batch(
        &self,
        mail_from: &str,
        rcpts: &[String],
        data: &[u8],
    ) -> Result<()> {
        if rcpts.is_empty() {
            return Ok(());
        }
        let ids = self
            .store
            .write_shared(mail_from, rcpts, data, now_unix())
            .await?;
        for id in ids {
            let _ = self.work_tx.send(id);
        }
        Ok(())
    }

    pub async fn depth(&self) -> Result<usize> {
        self.store.count_entries().await
    }

    pub fn config(&self) -> &QueueConfig {
        &self.config
    }

    pub fn store_location(&self) -> &std::path::Path {
        self.store.location()
    }

    async fn reload_disk_queue(&self) -> Result<()> {
        let ids = self.store.list_ids().await?;
        if ids.is_empty() {
            return Ok(());
        }
        let now = now_unix();
        let post_init = self.started_at.elapsed().unwrap_or_default() < self.config.post_init_delay;
        let min_start = if post_init {
            now + self.config.post_init_delay.as_secs()
        } else {
            now
        };

        let mut loaded = 0usize;
        for id in ids {
            let meta = match self.store.read_meta(&id).await {
                Ok(m) => m,
                Err(e) => {
                    warn!(%id, error = %e, "skipping corrupt queue entry");
                    self.store.remove(&id).await;
                    continue;
                }
            };
            if !self.body_exists(&id).await {
                warn!(%id, "queue body missing, removing");
                self.store.remove(&id).await;
                continue;
            }
            if self.config.is_expired(&meta) {
                self.fail_expired(&id, &meta).await;
                continue;
            }
            let run_at = meta.next_attempt_unix.max(min_start);
            self.schedule_id(&id, run_at).await;
            loaded += 1;
        }
        if loaded > 0 {
            info!(
                count = loaded,
                path = %self.store.location().display(),
                "loaded outbound queue entries from disk"
            );
        }
        Ok(())
    }

    async fn body_exists(&self, id: &str) -> bool {
        tokio::fs::metadata(self.store.location().join(format!("{id}.body")))
            .await
            .map(|m| m.is_file())
            .unwrap_or(false)
    }

    async fn schedule_id(&self, id: &str, run_at_unix: u64) {
        let id = id.to_string();
        let work_tx = self.work_tx.clone();
        let now = now_unix();
        if run_at_unix <= now {
            let _ = work_tx.send(id);
            return;
        }
        let delay_secs = run_at_unix.saturating_sub(now);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(delay_secs)).await;
            let _ = work_tx.send(id);
        });
    }

    async fn run_worker(self: Arc<Self>, mut work_rx: mpsc::UnboundedReceiver<String>) {
        let sem = Arc::new(Semaphore::new(self.config.max_parallelism));
        while let Some(id) = work_rx.recv().await {
            let q = Arc::clone(&self);
            let permit = match sem.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let _permit = permit;
                q.process_entry(&id).await;
            });
        }
    }

    async fn process_entry(&self, id: &str) {
        let (mut meta, data) = match self.store.load(id).await {
            Ok(v) => v,
            Err(e) => {
                warn!(%id, error = %e, "queue entry load failed");
                self.store.remove(id).await;
                return;
            }
        };

        if self.config.is_expired(&meta) {
            self.fail_expired(id, &meta).await;
            return;
        }

        meta.tries_count += 1;
        meta.last_attempt_unix = now_unix();

        let job = OutboundJob {
            mail_from: meta.mail_from.clone(),
            rcpt_to: meta.rcpt_to.clone(),
            data,
        };

        match deliver_remote(&self.ctx, &job).await {
            DeliveryOutcome::Success => {
                info!(%id, rcpt = %meta.rcpt_to, attempt = meta.tries_count, "outbound delivery succeeded");
                chatmail_db::increment_outbound();
                self.store.remove(id).await;
            }
            DeliveryOutcome::Permanent { reason } => {
                warn!(
                    %id,
                    rcpt = %meta.rcpt_to,
                    attempt = meta.tries_count,
                    error = %reason,
                    "outbound delivery permanent failure"
                );
                self.store.remove(id).await;
            }
            DeliveryOutcome::Temporary { reason } => {
                meta.last_error = Some(reason.clone());
                if meta.tries_count >= self.config.max_tries {
                    warn!(
                        %id,
                        rcpt = %meta.rcpt_to,
                        attempts = meta.tries_count,
                        error = %reason,
                        "outbound delivery exceeded max_tries, dropping"
                    );
                    self.store.remove(id).await;
                    return;
                }
                let delay = self.config.retry_delay(meta.tries_count);
                meta.next_attempt_unix = now_unix() + delay.as_secs();
                if let Err(e) = self.store.update_meta(&meta).await {
                    warn!(%id, error = %e, "failed to update queue meta");
                }
                warn!(
                    %id,
                    rcpt = %meta.rcpt_to,
                    attempt = meta.tries_count,
                    retry_in = ?delay,
                    error = %reason,
                    "outbound delivery failed, requeued"
                );
                self.schedule_id(id, meta.next_attempt_unix).await;
            }
        }
    }

    async fn fail_expired(&self, id: &str, meta: &super::store::QueueMeta) {
        let age_secs = now_unix().saturating_sub(meta.effective_queued_at());
        warn!(
            %id,
            rcpt = %meta.rcpt_to,
            age_secs,
            max_delivery = ?self.config.max_delivery_time,
            last_error = ?meta.last_error,
            "outbound delivery expired (max_delivery_time), marking failed and removing from queue"
        );
        self.store.remove(id).await;
    }
}
