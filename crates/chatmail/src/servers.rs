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
    if chatmail_config::settings_policy::admin_path_uses_existing_post_route(&path) {
        tracing::error!(
            %path,
            "admin API path conflicts with an existing POST route; not mounting"
        );
        return Ok(None);
    }

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

    /// Nesting the admin API on `POST /new` overlaps account registration.
    /// Axum panics while merging those routers, so a stored `/new` must not
    /// be mounted.
    #[tokio::test]
    async fn merging_the_admin_api_onto_post_new_panics_and_is_not_mounted() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let cfg = AppConfig::default();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let www = www_router(WwwState::new(
            pool.clone(),
            Arc::clone(&app),
            cfg.clone(),
            dir.path(),
        ));
        let state = AdminState::new(
            pool.clone(),
            Arc::clone(&app),
            cfg.clone(),
            dir.path().to_path_buf(),
            "example.org".into(),
            token.into(),
            None,
        );
        let nested = Router::new().nest("/new", admin_router(state));
        let merged = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            merge_http_routers(Some(nested), None, www)
        }));
        assert!(merged.is_err(), "POST /new is already account registration");

        chatmail_db::set_setting(&pool, chatmail_db::settings_keys::ADMIN_PATH, "/new")
            .await
            .unwrap();
        let mounted = build_admin_router(&cfg, dir.path(), token, pool, app, None)
            .await
            .unwrap();
        assert!(mounted.is_none());
    }

    // This service swaps the router and then signals done. The production
    // supervisor acknowledges the reload and only then restarts listeners
    // (supervisor.rs). `applied: true` on a real server means the remount was
    // accepted, not that the new path was probed and found reachable.
    #[tokio::test]
    async fn saving_admin_path_through_the_old_route_moves_the_live_router() {
        use axum::body::Body;
        use axum::http::Request;
        use axum::ServiceExt as _;
        use std::convert::Infallible;
        use std::task::{Context, Poll};
        use tower::Service;
        use tower::ServiceExt as _;

        #[derive(Clone)]
        struct Swap(Arc<tokio::sync::RwLock<Router>>);

        impl Service<Request<Body>> for Swap {
            type Response = axum::response::Response;
            type Error = Infallible;
            type Future = std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = std::result::Result<Self::Response, Self::Error>,
                        > + Send,
                >,
            >;

            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: Request<Body>) -> Self::Future {
                let routers = Arc::clone(&self.0);
                Box::pin(async move {
                    let router = routers.read().await.clone();
                    Ok(router.oneshot(req).await.unwrap_or_else(|err| match err {}))
                })
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let cfg = AppConfig::default();
        let reload_cfg = cfg.clone();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let (reload_tx, mut reload_rx) = tokio::sync::mpsc::channel(2);
        let rebuild_tx = reload_tx.clone();
        let initial = build_admin_router(
            &cfg,
            dir.path(),
            token,
            pool.clone(),
            Arc::clone(&app),
            Some(reload_tx),
        )
        .await
        .unwrap()
        .unwrap();
        let slot = Arc::new(tokio::sync::RwLock::new(initial));
        let rebuild = Arc::clone(&slot);
        let rebuild_pool = pool.clone();
        let rebuild_app = Arc::clone(&app);
        let rebuild_dir = dir.path().to_path_buf();
        tokio::spawn(async move {
            while let Some(req) = reload_rx.recv().await {
                let next = build_admin_router(
                    &reload_cfg,
                    &rebuild_dir,
                    token,
                    rebuild_pool.clone(),
                    Arc::clone(&rebuild_app),
                    Some(rebuild_tx.clone()),
                )
                .await
                .unwrap()
                .unwrap();
                *rebuild.write().await = next;
                if let Some(done) = req.done {
                    let _ = done.send(Ok(()));
                }
            }
        });

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, Swap(slot).into_make_service())
                .await
                .unwrap();
        });
        let client = reqwest::Client::new();
        let auth = format!("Bearer {token}");
        let saved: serde_json::Value = client
            .post(format!("http://{addr}/api/admin"))
            .json(&serde_json::json!({
                "method": "POST",
                "resource": "/admin/settings/admin_path",
                "headers": { "Authorization": auth },
                "body": { "action": "set", "value": "/api/secret" }
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(saved["status"], 200, "{saved}");
        assert_eq!(saved["body"]["value"], "/api/secret");
        assert_eq!(saved["body"]["applied"], true, "{saved}");

        let authed: serde_json::Value = client
            .post(format!("http://{addr}/api/secret"))
            .json(&serde_json::json!({
                "method": "GET",
                "resource": "/admin/settings",
                "headers": { "Authorization": auth }
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(authed["status"], 200, "{authed}");
        let old = client
            .post(format!("http://{addr}/api/admin"))
            .json(&serde_json::json!({
                "method": "GET",
                "resource": "/admin/settings",
                "headers": { "Authorization": auth }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(old.status(), reqwest::StatusCode::NOT_FOUND);

        let reset: serde_json::Value = client
            .post(format!("http://{addr}/api/secret"))
            .json(&serde_json::json!({
                "method": "POST",
                "resource": "/admin/settings/admin_path",
                "headers": { "Authorization": auth },
                "body": { "action": "reset" }
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(reset["status"], 200, "{reset}");
        assert_eq!(reset["body"]["value"], "/api/admin");
        assert_eq!(reset["body"]["applied"], true, "{reset}");
        let home: serde_json::Value = client
            .post(format!("http://{addr}/api/admin"))
            .json(&serde_json::json!({
                "method": "GET",
                "resource": "/admin/settings",
                "headers": { "Authorization": auth }
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(home["status"], 200, "{home}");
        task.abort();
    }
}
