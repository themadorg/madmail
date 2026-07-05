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

pub mod auth;
pub mod events;
pub mod federation_size;
pub mod flusher;
pub mod listener_ports;
pub mod message_size;
pub mod policy;
pub mod quota;
pub mod reload;
pub mod silent_dismiss;
pub mod tracker;

use std::sync::Arc;

use chatmail_config::AppConfig;
use chatmail_db::DbPool;
use chatmail_push::{push_runtime_enabled, PushNotifier};
use chatmail_webhooks::WebhookDispatcher;
use chatmail_storage::{MailboxStore, StoragePolicy};
use chatmail_types::Result;
use dashmap::DashMap;
use tokio::sync::Mutex;

pub use auth::AuthCache;
pub use events::{EventBus, NewMessageEvent};
pub use federation_size::FederationSizeLimit;
pub use flusher::{flush_federation_stats, flush_modseq, start_flusher, FlusherHandle};
pub use listener_ports::{ListenerPorts, ListenerPortsStore};
pub use message_size::MessageSizeLimit;
pub use policy::{FederationPolicyCache, PolicyMode};
pub use quota::QuotaCache;
pub use reload::{ReloadRequest, ReloadScope};
pub use silent_dismiss::FederationSilentDismissCache;
pub use tracker::{FederationTracker, ServerStat};

/// Shared hot-path state hydrated at boot.
#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<AuthCache>,
    pub message_size: Arc<MessageSizeLimit>,
    pub federation_size: Arc<FederationSizeLimit>,
    pub quota: Arc<QuotaCache>,
    pub federation_tracker: Arc<FederationTracker>,
    pub federation_policy: Arc<FederationPolicyCache>,
    pub federation_silent_dismiss: Arc<FederationSilentDismissCache>,
    pub mailbox_store: Arc<MailboxStore>,
    pub events: Arc<EventBus>,
    /// FCM/APNS wake-up via Delta Chat notification proxy.
    pub push: Arc<PushNotifier>,
    /// Operator-configured HTTPS webhooks (registration, quota cap).
    pub webhooks: Arc<WebhookDispatcher>,
    /// Bound listener ports (IMAP, etc.) for admin status / `ss` probes.
    pub listener_ports: Arc<ListenerPortsStore>,
    /// Per-user mutexes so concurrent JIT logins coalesce on one DB create.
    pub jit_flights: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl AppState {
    /// Dev/tests: use [`chatmail_config::DEFAULT_QUOTA_BYTES`] (1 GiB, same as Madmail).
    pub fn new(state_dir: impl AsRef<std::path::Path>, pool: DbPool) -> Self {
        Self::with_default_quota(state_dir, chatmail_config::DEFAULT_QUOTA_BYTES, pool)
    }

    pub fn with_default_quota(
        state_dir: impl AsRef<std::path::Path>,
        default_quota_bytes: u64,
        pool: DbPool,
    ) -> Self {
        Self::with_quota_and_message_limit(
            state_dir,
            default_quota_bytes,
            &AppConfig::default(),
            pool,
        )
    }

    pub fn with_quota_and_message_limit(
        state_dir: impl AsRef<std::path::Path>,
        default_quota_bytes: u64,
        config: &AppConfig,
        pool: DbPool,
    ) -> Self {
        let state_dir = state_dir.as_ref().to_path_buf();
        let queue_dir = state_dir.join("pending_notifications");
        let push = Arc::new(PushNotifier::new(pool.clone(), queue_dir, None));
        let webhooks = Arc::new(WebhookDispatcher::new(pool));
        Self {
            auth: Arc::new(AuthCache::new()),
            message_size: Arc::new(MessageSizeLimit::new(config)),
            federation_size: Arc::new(FederationSizeLimit::new(config)),
            quota: Arc::new(QuotaCache::with_webhooks(
                default_quota_bytes,
                Some(Arc::clone(&webhooks)),
            )),
            federation_tracker: Arc::new(FederationTracker::new()),
            federation_policy: Arc::new(FederationPolicyCache::new()),
            federation_silent_dismiss: Arc::new(FederationSilentDismissCache::new()),
            mailbox_store: Arc::new(MailboxStore::with_policy(
                state_dir,
                StoragePolicy::from_config(
                    config.mail_fsync.as_deref(),
                    config.blob_dedup.as_deref(),
                ),
            )),
            events: Arc::new(EventBus::new()),
            push,
            webhooks,
            listener_ports: Arc::new(ListenerPortsStore::new()),
            jit_flights: Arc::new(DashMap::new()),
        }
    }

    /// Serialize JIT account creation for the same username across concurrent logins.
    pub fn jit_flight(&self, user: &str) -> Arc<Mutex<()>> {
        self.jit_flights
            .entry(user.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub fn check_message_size(&self, len: usize) -> Result<()> {
        if len as u64 > self.message_size.effective() {
            return Err(chatmail_types::ChatmailError::message_too_large());
        }
        Ok(())
    }

    pub fn check_federation_size(&self, len: usize) -> Result<()> {
        if len as u64 > self.federation_size.effective() {
            return Err(chatmail_types::ChatmailError::message_too_large());
        }
        Ok(())
    }

    /// Queue XDELTAPUSH device notifications after inbound mail (skips self-sent).
    pub async fn notify_inbound_push(&self, pool: &DbPool, mail_from: &str, rcpt: &str) {
        if push_runtime_enabled(pool).await.unwrap_or(false)
            && !mail_from.eq_ignore_ascii_case(rcpt)
        {
            self.push.notify_inbound(rcpt);
        }
    }

    pub async fn hydrate(&self, pool: &DbPool, config: &AppConfig) -> Result<()> {
        self.auth.hydrate(pool).await?;
        self.message_size.hydrate(pool, config).await?;
        self.federation_size.hydrate(pool, config).await?;
        self.quota.hydrate(pool, &self.mailbox_store).await?;
        self.federation_policy.hydrate(pool).await?;
        self.federation_silent_dismiss.hydrate(pool).await?;
        self.federation_tracker.hydrate(pool).await?;
        // Seed durable INBOX modseq so change-ids stay monotonic across restarts.
        for (user, modseq) in chatmail_db::load_all_modseq(pool).await? {
            self.events.seed_inbox_version(&user, modseq.max(0) as u64);
        }
        Ok(())
    }

    pub fn start_flusher(&self, pool: DbPool) -> FlusherHandle {
        start_flusher(
            pool,
            Arc::clone(&self.federation_tracker),
            Arc::clone(&self.events),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_storage::FsyncMode;

    /// P11-UT24: AppState wires storage policy from maddy.conf mail_fsync / blob_dedup.
    #[tokio::test]
    async fn p11_ut24_appstate_applies_storage_policy_from_config() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let cfg = AppConfig {
            mail_fsync: Some("optimized".into()),
            blob_dedup: Some("off".into()),
            ..AppConfig::default()
        };

        let state = AppState::with_quota_and_message_limit(
            "/tmp/chatmail-test-state",
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &cfg,
            pool.clone(),
        );
        let policy = state.mailbox_store.policy();
        assert_eq!(policy.fsync_mode, FsyncMode::Optimized);
        assert!(!policy.cas_enabled);

        let default = AppState::new("/tmp/chatmail-default", pool);
        assert_eq!(default.mailbox_store.policy().fsync_mode, FsyncMode::Always);
        assert!(default.mailbox_store.policy().cas_enabled);
    }
}
