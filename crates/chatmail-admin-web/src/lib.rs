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
use chatmail_db::{get_setting, set_setting, DbPool};
use chatmail_types::Result;

pub use serve::{admin_web_router, AdminWebState};

/// Default mount path when `admin-web enable` is used and no path is configured.
pub const DEFAULT_ADMIN_WEB_PATH: &str = "/admin";

/// Resolve URL path for the admin-web SPA (`admin_web_path` in config, `__ADMIN_WEB_PATH__` in DB).
///
/// Falls back to [`DEFAULT_ADMIN_WEB_PATH`] when neither source sets a path.
pub async fn resolve_admin_web_path(file_config: &AppConfig, pool: &DbPool) -> Option<String> {
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

/// Persist [`DEFAULT_ADMIN_WEB_PATH`] when enabling and no path is configured yet.
pub async fn ensure_default_admin_web_path(
    file_config: &AppConfig,
    pool: &DbPool,
) -> Result<()> {
    if file_config.admin_web_path.is_some() {
        return Ok(());
    }
    if let Ok(Some(v)) = get_setting(pool, ADMIN_WEB_PATH).await {
        if !v.is_empty() {
            return Ok(());
        }
    }
    set_setting(pool, ADMIN_WEB_PATH, DEFAULT_ADMIN_WEB_PATH).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::init_memory_db;

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
}

fn normalize_prefix(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
    }
    let path = path.trim_end_matches('/');
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

/// Build router for the admin-web SPA (mounted at resolved path, usually `/admin`).
pub async fn router_if_configured(
    file_config: &AppConfig,
    pool: DbPool,
) -> Result<Option<axum::Router>> {
    let Some(prefix) = resolve_admin_web_path(file_config, &pool).await else {
        return Ok(None);
    };
    let state = AdminWebState::new(pool, prefix.clone());
    tracing::info!(path = %prefix, "admin web UI mounted (embedded SPA)");
    Ok(Some(admin_web_router(state)))
}
