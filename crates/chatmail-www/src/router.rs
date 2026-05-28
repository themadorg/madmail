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
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use axum::routing::{delete, get, post};
use axum::Router;
use chatmail_config::AppConfig;
use chatmail_state::AppState;
use chatmail_db::DbPool;

use crate::assets::{embedded_asset_bytes, external_asset_bytes, preload_embedded_www};
use crate::context_cache::{SharedWwwContextCache, WwwContextCache};
use crate::handlers;
use crate::template::TemplateEngine;
use crate::webimap;

#[derive(Clone)]
pub struct WwwState {
    pub pool: DbPool,
    pub app: Arc<AppState>,
    pub config: AppConfig,
    pub templates: Arc<TemplateEngine>,
    pub mail_domain: String,
    /// Domains accepted for local delivery (WebIMAP send + SMTP).
    pub local_domains: Vec<String>,
    /// External www root (`chatmail { www_dir }` / `html-serve`). Unset = embedded default site.
    pub www_dir: Option<PathBuf>,
    /// Cached DB-backed template fields (Madmail `hydrateCache`, 5s TTL).
    pub context_cache: SharedWwwContextCache,
    /// Embedded static assets preloaded into RAM (`www_dir` unset only).
    asset_cache: Arc<RwLock<HashMap<String, Arc<[u8]>>>>,
}

impl WwwState {
    pub fn new(pool: DbPool, app: Arc<AppState>, config: AppConfig) -> Self {
        let mail_domain = config.effective_registration_domain(None);
        let hostname = config
            .hostname
            .clone()
            .unwrap_or_else(|| "127.0.0.1".into());
        let local_domains = config.effective_local_domains(&hostname);
        let www_dir = config.www_dir.clone();
        let templates = Arc::new(TemplateEngine::from_config(&config));
        let asset_cache = Arc::new(RwLock::new(HashMap::new()));
        if www_dir.is_none() {
            preload_embedded_www(&asset_cache);
            tracing::debug!("www: default site from embedded RAM (no www_dir)");
        }
        Self {
            pool,
            app,
            config,
            templates,
            mail_domain,
            local_domains,
            www_dir,
            context_cache: Arc::new(WwwContextCache::new()),
            asset_cache,
        }
    }

    /// CSS/JS/SVG: embedded default = RAM only; external `www_dir` = live disk.
    pub fn load_asset(&self, path: &str) -> Option<Arc<[u8]>> {
        if let Some(ref dir) = self.www_dir {
            return external_asset_bytes(path, dir).map(|v| Arc::from(v));
        }
        if let Some(b) = self.asset_cache.read().ok()?.get(path) {
            return Some(Arc::clone(b));
        }
        let arc = embedded_asset_bytes(path)?;
        if let Ok(mut guard) = self.asset_cache.write() {
            guard.insert(path.to_string(), Arc::clone(&arc));
        }
        Some(arc)
    }

    /// Default webpage baked into the binary (no `www_dir` in config).
    pub fn uses_embedded_www(&self) -> bool {
        self.www_dir.is_none()
    }

    /// Exported tree via `html-export` / `html-serve` (live disk reload).
    pub fn uses_external_www(&self) -> bool {
        self.www_dir.is_some()
    }
}

/// Public Madmail-compatible web UI (index, docs, /new, `/madmail` binary, static assets).
pub fn www_router(state: WwwState) -> Router {
    Router::new()
        .route("/madmail", get(handlers::binary_download))
        .route("/new", post(handlers::new_account))
        .route(
            "/webimap/send",
            post(handlers::webimap_send).options(webimap::options),
        )
        .route(
            "/websmtp/send",
            post(handlers::webimap_send).options(webimap::options),
        )
        .route(
            "/webimap/mailboxes",
            get(webimap::mailboxes).options(webimap::options),
        )
        .route(
            "/webimap/messages",
            get(webimap::messages).options(webimap::options),
        )
        .route(
            "/webimap/message/{uid}",
            get(webimap::message_get)
                .delete(webimap::message_delete)
                .options(webimap::options),
        )
        .route(
            "/webimap/messages/{mailbox}/{uid}",
            delete(webimap::messages_delete).options(webimap::options),
        )
        .route(
            "/webimap/message/flags",
            post(webimap::message_flags).options(webimap::options),
        )
        .route("/webimap/ws", get(webimap::websocket))
        .route("/share", get(handlers::share_get).post(handlers::share_post))
        .route("/app", get(handlers::app_page))
        .route("/docs", get(handlers::docs_redirect))
        .route("/docs/", get(handlers::docs_index))
        .route("/docs/{*path}", get(handlers::docs_path))
        .route("/inv/{*token}", get(handlers::invite_page))
        .route("/", get(handlers::index))
        .route("/{*path}", get(handlers::catch_all))
        .with_state(state)
}
