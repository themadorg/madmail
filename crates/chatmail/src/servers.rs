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
    let api_path = chatmail_db::get_setting(&pool, chatmail_db::settings_keys::ADMIN_PATH)
        .await?
        .or_else(|| file_config.admin_path.clone())
        .unwrap_or_else(|| "/api/admin".into());
    let api_path = chatmail_config::settings_policy::validate("admin_path", &api_path)?;
    let web_path = chatmail_admin_web::resolve_admin_web_path(file_config, &pool)
        .await
        .unwrap_or_else(|| "/admin".into());
    let web_path = chatmail_config::settings_policy::validate("admin_web_path", &web_path)?;
    if api_path == web_path
        || api_path.starts_with(&format!("{web_path}/"))
        || web_path.starts_with(&format!("{api_path}/"))
    {
        return Err(chatmail_types::ChatmailError::config(
            "admin API and web paths must not overlap",
        ));
    }
    let admin_extra = build_admin_router(
        file_config,
        state_dir,
        admin_token,
        pool.clone(),
        Arc::clone(&app),
        reload_tx,
    )
    .await?;
    let admin_web_extra =
        chatmail_admin_web::router_if_configured(file_config, pool.clone()).await?;
    let www_extra = www_router(WwwState::new(pool, app, file_config.clone(), state_dir));
    Ok(merge_http_routers(admin_extra, admin_web_extra, www_extra))
}

pub(crate) async fn build_admin_router(
    file_config: &AppConfig,
    state_dir: &Path,
    admin_token: &str,
    pool: DbPool,
    app: Arc<AppState>,
    reload_tx: Option<mpsc::Sender<ReloadRequest>>,
) -> Result<Option<Router>> {
    let disabled = file_config
        .admin_token
        .as_deref()
        .is_some_and(|t| t.eq_ignore_ascii_case("disabled"));
    if disabled {
        tracing::info!("admin API disabled via config (admin_token disabled)");
        return Ok(None);
    }

    let token = file_config
        .admin_token
        .clone()
        .filter(|t| !t.eq_ignore_ascii_case("disabled"))
        .unwrap_or_else(|| admin_token.to_string());

    let path = chatmail_db::get_setting(&pool, chatmail_db::settings_keys::ADMIN_PATH)
        .await?
        .or_else(|| file_config.admin_path.clone())
        .unwrap_or_else(|| "/api/admin".into());
    let path = chatmail_config::settings_policy::validate("admin_path", &path)?;
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
    Ok(Some(Router::new().nest(&path, inner)))
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

#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[tokio::test]
    async fn admin_database_path_remount_preserves_authentication() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let cfg = AppConfig::default();
        let mut policy = chatmail_config::SettingsPolicy::default();
        policy
            .insert(
                "admin_web_enabled",
                "false",
                "environment:CHATMAIL_ADMIN_WEB_ENABLED",
            )
            .unwrap();
        let pool = pool.with_policy(policy);
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        chatmail_db::set_setting(&pool, chatmail_db::settings_keys::ADMIN_PATH, "/api/custom")
            .await
            .unwrap();
        let router = build_admin_router(&cfg, dir.path(), token, pool.clone(), app, None)
            .await
            .unwrap()
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        let body = serde_json::json!({"method":"GET","resource":"/admin/settings","headers":{"Authorization":format!("Bearer {token}")}});
        let response: serde_json::Value = client
            .post(format!("http://{addr}/api/custom"))
            .json(&body)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(response["status"], 200);
        let response: serde_json::Value = client
            .post(format!("http://{addr}/api/custom"))
            .json(&serde_json::json!({"method":"GET","resource":"/admin/settings"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(response["status"], 401);
        let old = client
            .post(format!("http://{addr}/api/admin"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(old.status(), reqwest::StatusCode::NOT_FOUND);
        let rejected: serde_json::Value = client.post(format!("http://{addr}/api/custom"))
            .json(&serde_json::json!({"method":"POST","resource":"/admin/services/admin_web","headers":{"Authorization":format!("Bearer {token}")},"body":{"action":"enable"}}))
            .send().await.unwrap().json().await.unwrap();
        assert_eq!(rejected["status"], 409);
        assert_eq!(rejected["error_code"], "configuration_locked");
        assert!(chatmail_db::settings::get_stored_setting(
            &pool,
            chatmail_db::settings_keys::ADMIN_WEB_ENABLED
        )
        .await
        .unwrap()
        .is_none());
        task.abort();
    }
}
