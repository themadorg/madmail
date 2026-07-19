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

//! Cached www template settings (Madmail `hydrateCache` — refresh at most every 5s).

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chatmail_config::{AppConfig, DbMailPorts};
use chatmail_db::{db_ports_from_settings, get_settings_many, settings_keys, DbPool};
use chatmail_shadowsocks::{resolve_runtime_from_settings, ShadowsocksRuntime};
use chatmail_types::Result;
use tokio::sync::RwLock;

/// Keys loaded in one batch for HTML pages (ports, toggles, language, Shadowsocks).
pub const WWW_SETTINGS_KEYS: &[&str] = &[
    settings_keys::LANGUAGE,
    settings_keys::REGISTRATION_OPEN,
    settings_keys::JIT_REGISTRATION_ENABLED,
    settings_keys::SMTP_PORT,
    settings_keys::SUBMISSION_PORT,
    settings_keys::SUBMISSION_TLS_PORT,
    settings_keys::IMAP_PORT,
    settings_keys::IMAP_TLS_PORT,
    settings_keys::DCLOGIN_IMAP_SECURITY,
    settings_keys::DCLOGIN_SMTP_SECURITY,
    settings_keys::HTTP_PORT,
    settings_keys::HTTPS_PORT,
    settings_keys::SS_PASSWORD,
    settings_keys::SS_CIPHER,
    settings_keys::SS_PORT,
    settings_keys::SS_ENABLED,
];

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub(crate) struct Snapshot {
    pub(crate) language: String,
    pub(crate) registration_open: bool,
    pub(crate) jit_registration_enabled: bool,
    pub(crate) db_ports: DbMailPorts,
    pub(crate) ss_runtime: Option<ShadowsocksRuntime>,
}

pub struct WwwContextCache {
    inner: RwLock<Option<Cached>>,
}

struct Cached {
    at: Instant,
    snap: Snapshot,
}

impl Default for WwwContextCache {
    fn default() -> Self {
        Self::new()
    }
}

impl WwwContextCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(None),
        }
    }

    pub async fn ensure_fresh(
        &self,
        pool: &DbPool,
        config: &AppConfig,
        state_dir: &Path,
    ) -> Result<()> {
        let stale = {
            let guard = self.inner.read().await;
            guard
                .as_ref()
                .is_none_or(|c| c.at.elapsed() >= REFRESH_INTERVAL)
        };
        if !stale {
            return Ok(());
        }
        let snap = Self::load_snapshot(pool, config, state_dir).await?;
        *self.inner.write().await = Some(Cached {
            at: Instant::now(),
            snap,
        });
        Ok(())
    }

    pub(crate) async fn snapshot(&self) -> Option<Snapshot> {
        self.inner.read().await.as_ref().map(|c| c.snap.clone())
    }

    async fn load_snapshot(
        pool: &DbPool,
        config: &AppConfig,
        state_dir: &Path,
    ) -> Result<Snapshot> {
        let map = get_settings_many(pool, WWW_SETTINGS_KEYS).await?;
        let db_ports = db_ports_from_settings(&map);

        let language = map
            .get(settings_keys::LANGUAGE)
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                config
                    .language
                    .as_ref()
                    .map(|l| l.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| "en".into());

        let registration_open = bool_setting(&map, settings_keys::REGISTRATION_OPEN, true);
        let jit_registration_enabled =
            bool_setting(&map, settings_keys::JIT_REGISTRATION_ENABLED, true);

        let mail_domain = config.effective_registration_domain(None);
        let ss_runtime = if config.ss_configured() {
            Some(resolve_runtime_from_settings(
                config,
                &mail_domain,
                state_dir,
                &db_ports,
                &map,
            )?)
        } else {
            None
        };

        Ok(Snapshot {
            language,
            registration_open,
            jit_registration_enabled,
            db_ports,
            ss_runtime,
        })
    }
}

fn bool_setting(map: &std::collections::HashMap<String, String>, key: &str, default: bool) -> bool {
    map.get(key)
        .map(|v| chatmail_config::parse_bool_str(v))
        .unwrap_or(default)
}

pub type SharedWwwContextCache = Arc<WwwContextCache>;
