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

use std::path::Path;
use std::sync::Arc;

use axum::Router;
use chatmail_admin::{admin_router, AdminState};
use chatmail_config::AppConfig;
use chatmail_db::DbPool;
use chatmail_state::{AppState, ReloadRequest};
use chatmail_types::Result;
use chatmail_www::{www_router, WwwState};
use tokio::sync::mpsc;

use crate::supervisor::ServerSupervisor;

pub async fn start_servers(
    pool: DbPool,
    app: Arc<AppState>,
    file_config: &AppConfig,
    state_dir: &Path,
    admin_token: &str,
) -> Result<(ServerSupervisor, mpsc::Sender<ReloadRequest>)> {
    ServerSupervisor::start(pool, app, file_config, state_dir, admin_token).await
}

/// Admin API + embedded admin-web SPA + www routes merged for HTTP listeners.
pub(crate) async fn build_http_extra(
    file_config: &AppConfig,
    state_dir: &Path,
    admin_token: &str,
    pool: DbPool,
    app: Arc<AppState>,
    reload_tx: Option<mpsc::Sender<ReloadRequest>>,
) -> Result<Option<Router>> {
    let admin_extra = build_admin_router(
        file_config,
        state_dir,
        admin_token,
        pool.clone(),
        Arc::clone(&app),
        reload_tx,
    );
    let admin_web_extra =
        chatmail_admin_web::router_if_configured(file_config, pool.clone()).await?;
    let www_extra = www_router(WwwState::new(pool, app, file_config.clone(), state_dir));
    Ok(merge_http_routers(admin_extra, admin_web_extra, www_extra))
}

pub(crate) fn build_admin_router(
    file_config: &AppConfig,
    state_dir: &Path,
    admin_token: &str,
    pool: DbPool,
    app: Arc<AppState>,
    reload_tx: Option<mpsc::Sender<ReloadRequest>>,
) -> Option<Router> {
    let disabled = file_config
        .admin_token
        .as_deref()
        .is_some_and(|t| t.eq_ignore_ascii_case("disabled"));
    if disabled {
        tracing::info!("admin API disabled via config (admin_token disabled)");
        return None;
    }

    let token = file_config
        .admin_token
        .clone()
        .filter(|t| !t.eq_ignore_ascii_case("disabled"))
        .unwrap_or_else(|| admin_token.to_string());

    let path = std::env::var("CHATMAIL_ADMIN_PATH")
        .ok()
        .or_else(|| file_config.admin_path.clone())
        .unwrap_or_else(|| "/api/admin".into());
    let path = normalize_admin_path(&path);

    let hostname = file_config
        .hostname
        .clone()
        .unwrap_or_else(|| "127.0.0.1".into());
    let mail_domain = file_config.effective_registration_domain(Some(&hostname));
    let state = AdminState::new(
        pool,
        app,
        file_config.clone(),
        state_dir.to_path_buf(),
        mail_domain,
        token,
        reload_tx,
    );
    let inner = admin_router(state);
    tracing::info!(%path, "admin API mounted");
    Some(Router::new().nest(&path, inner))
}

pub(crate) fn merge_http_routers(
    admin: Option<Router>,
    admin_web: Option<Router>,
    www: Router,
) -> Option<Router> {
    // Admin + admin-web before www `/{*path}` catch-all.
    let mut extra = www;
    if let Some(w) = admin_web {
        extra = w.merge(extra);
    }
    if let Some(a) = admin {
        extra = a.merge(extra);
    }
    Some(extra)
}

/// Local delivery accepts `user@a.com`, `user@[1.1.1.1]`, and legacy `user@localhost` in dev.
pub(crate) fn extend_dev_local_aliases(domains: &mut Vec<String>) {
    use std::collections::HashSet;
    let mut set: HashSet<String> = domains.iter().cloned().collect();
    for alias in ["localhost", "127.0.0.1", "[127.0.0.1]"] {
        for form in chatmail_types::domain_forms(alias) {
            set.insert(form);
        }
    }
    let mut v: Vec<_> = set.into_iter().collect();
    v.sort();
    *domains = v;
}

fn normalize_admin_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return "/api/admin".into();
    }
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}
