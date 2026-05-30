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
pub mod flusher;
pub mod listener_ports;
pub mod message_size;
pub mod policy;
pub mod quota;
pub mod silent_dismiss;
pub mod tracker;

use std::sync::Arc;

use chatmail_config::AppConfig;
use chatmail_db::DbPool;
use chatmail_storage::MailboxStore;
use chatmail_types::Result;
use dashmap::DashMap;
use tokio::sync::Mutex;

pub use auth::AuthCache;
pub use events::{EventBus, NewMessageEvent};
pub use flusher::{flush_federation_stats, start_flusher, FlusherHandle};
pub use listener_ports::{ListenerPorts, ListenerPortsStore};
pub use message_size::MessageSizeLimit;
pub use policy::{FederationPolicyCache, PolicyMode};
pub use quota::QuotaCache;
pub use silent_dismiss::FederationSilentDismissCache;
pub use tracker::{FederationTracker, ServerStat};

/// Shared hot-path state hydrated at boot.
#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<AuthCache>,
    pub message_size: Arc<MessageSizeLimit>,
    pub quota: Arc<QuotaCache>,
    pub federation_tracker: Arc<FederationTracker>,
    pub federation_policy: Arc<FederationPolicyCache>,
    pub federation_silent_dismiss: Arc<FederationSilentDismissCache>,
    pub mailbox_store: Arc<MailboxStore>,
    pub events: Arc<EventBus>,
    /// Bound listener ports (IMAP, etc.) for admin status / `ss` probes.
    pub listener_ports: Arc<ListenerPortsStore>,
    /// Per-user mutexes so concurrent JIT logins coalesce on one DB create.
    pub jit_flights: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl AppState {
    /// Dev/tests: use [`chatmail_config::DEFAULT_QUOTA_BYTES`] (1 GiB, same as Madmail).
    pub fn new(state_dir: impl AsRef<std::path::Path>) -> Self {
        Self::with_default_quota(state_dir, chatmail_config::DEFAULT_QUOTA_BYTES)
    }

    pub fn with_default_quota(
        state_dir: impl AsRef<std::path::Path>,
        default_quota_bytes: u64,
    ) -> Self {
        Self::with_quota_and_message_limit(state_dir, default_quota_bytes, &AppConfig::default())
    }

    pub fn with_quota_and_message_limit(
        state_dir: impl AsRef<std::path::Path>,
        default_quota_bytes: u64,
        config: &AppConfig,
    ) -> Self {
        let state_dir = state_dir.as_ref().to_path_buf();
        Self {
            auth: Arc::new(AuthCache::new()),
            message_size: Arc::new(MessageSizeLimit::new(config)),
            quota: Arc::new(QuotaCache::new(default_quota_bytes)),
            federation_tracker: Arc::new(FederationTracker::new()),
            federation_policy: Arc::new(FederationPolicyCache::new()),
            federation_silent_dismiss: Arc::new(FederationSilentDismissCache::new()),
            mailbox_store: Arc::new(MailboxStore::new(state_dir)),
            events: Arc::new(EventBus::new()),
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

    pub async fn hydrate(&self, pool: &DbPool, config: &AppConfig) -> Result<()> {
        self.auth.hydrate(pool).await?;
        self.message_size.hydrate(pool, config).await?;
        self.quota.hydrate(pool, &self.mailbox_store).await?;
        self.federation_policy.hydrate(pool).await?;
        self.federation_silent_dismiss.hydrate(pool).await?;
        self.federation_tracker.hydrate(pool).await?;
        Ok(())
    }

    pub fn start_flusher(&self, pool: DbPool) -> FlusherHandle {
        start_flusher(pool, Arc::clone(&self.federation_tracker))
    }
}
