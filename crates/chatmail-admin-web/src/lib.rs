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

mod assets;
mod patch;
mod serve;

use chatmail_config::AppConfig;
use chatmail_db::settings_keys::ADMIN_WEB_PATH;
use chatmail_db::{get_setting, DbPool};
use chatmail_types::Result;

pub use serve::{admin_web_router, AdminWebState};

/// Default mount path when `admin-web enable` is used and no path is configured.
pub const DEFAULT_ADMIN_WEB_PATH: &str = "/admin";

/// Contiguous `madmail-admin-web-generation:<gitlink>` identity baked into servers.
pub const ADMIN_WEB_GENERATION_MARKER: &str = concat!(
    "madmail-admin-web-generation:",
    env!("MADMAIL_ADMIN_WEB_GENERATION")
);

/// Keep [`ADMIN_WEB_GENERATION_MARKER`] reachable so the linker retains it.
pub fn retain_admin_web_generation() {
    let marker = ADMIN_WEB_GENERATION_MARKER;
    if std::hint::black_box(marker.as_ptr()).is_null() {
        std::process::abort();
    }
}

/// Normalize an admin-web mount path (`/xxx`, trims trailing slash). Returns `None` if empty.
pub fn normalize_admin_web_path(path: &str) -> Option<String> {
    chatmail_config::settings_policy::validate("admin_web_path", path).ok()
}

fn normalize_prefix(path: &str) -> String {
    normalize_admin_web_path(path).unwrap_or_default()
}

/// Resolve URL path for the admin-web SPA (`admin_web_path` in config, `__ADMIN_WEB_PATH__` in DB).
///
/// Falls back to [`DEFAULT_ADMIN_WEB_PATH`] when neither source sets a path.
pub async fn resolve_admin_web_path(file_config: &AppConfig, pool: &DbPool) -> Option<String> {
    if let Some(value) = pool.policy().0.get(ADMIN_WEB_PATH) {
        return normalize_admin_web_path(&value.value);
    }
    let protected = file_config.deployment_policy().ok()?;
    if let Some(value) = protected.0.get(ADMIN_WEB_PATH) {
        return normalize_admin_web_path(&value.value);
    }
    let mut path = file_config.admin_web_path.clone();
    if let Ok(Some(v)) = get_setting(pool, ADMIN_WEB_PATH).await {
        if !v.is_empty() {
            path = Some(v);
        }
    }
    let path = path.unwrap_or_else(|| DEFAULT_ADMIN_WEB_PATH.to_string());
    let normalized = normalize_prefix(&path);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

/// Build router for the admin-web SPA (mounted at resolved path, usually `/admin`).
pub async fn router_if_configured(
    file_config: &AppConfig,
    pool: DbPool,
) -> Result<Option<axum::Router>> {
    retain_admin_web_generation();
    let Some(prefix) = resolve_admin_web_path(file_config, &pool).await else {
        return Ok(None);
    };
    let state = AdminWebState::new(pool, prefix.clone());
    tracing::info!(path = %prefix, "admin web UI mounted (embedded SPA)");
    Ok(Some(admin_web_router(state)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::{init_memory_db, set_setting};

    #[tokio::test]
    async fn resolve_defaults_to_admin() {
        let pool = init_memory_db().await.unwrap();
        let cfg = AppConfig::default();
        let path = resolve_admin_web_path(&cfg, &pool).await.unwrap();
        assert_eq!(path, "/admin");
    }

    #[tokio::test]
    async fn db_path_override_wins() {
        let pool = init_memory_db().await.unwrap();
        set_setting(&pool, ADMIN_WEB_PATH, "/panel").await.unwrap();
        let cfg = AppConfig::default();
        let path = resolve_admin_web_path(&cfg, &pool).await.unwrap();
        assert_eq!(path, "/panel");
    }

    #[test]
    fn normalize_adds_leading_slash_and_trims_trailing() {
        assert_eq!(normalize_admin_web_path("xxx/"), Some("/xxx".into()));
        assert_eq!(normalize_admin_web_path("/xxx"), Some("/xxx".into()));
        assert_eq!(normalize_admin_web_path("  "), None);
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    #[tokio::test]
    async fn reset_enable_then_add_file_path_does_not_seed_database() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        chatmail_db::set_setting(&pool, ADMIN_WEB_PATH, "/old")
            .await
            .unwrap();
        chatmail_db::delete_setting(&pool, ADMIN_WEB_PATH)
            .await
            .unwrap();
        chatmail_db::set_setting(&pool, chatmail_db::settings_keys::ADMIN_WEB_ENABLED, "true")
            .await
            .unwrap();
        assert!(chatmail_db::get_setting(&pool, ADMIN_WEB_PATH)
            .await
            .unwrap()
            .is_none());
        let cfg = AppConfig {
            admin_web_path: Some("/from-file".into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_admin_web_path(&cfg, &pool).await.as_deref(),
            Some("/from-file")
        );
        chatmail_db::set_setting(&pool, ADMIN_WEB_PATH, "/stale-db")
            .await
            .unwrap();
        assert_eq!(
            resolve_admin_web_path(&cfg, &pool).await.as_deref(),
            Some("/from-file")
        );
    }
}
