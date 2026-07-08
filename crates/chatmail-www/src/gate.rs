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

//! Madmail-compatible feature gates for WebIMAP / WebSMTP (`__WEBIMAP_ENABLED__`, `__WEBSMTP_ENABLED__`).

use axum::http::StatusCode;
use axum::response::Response;
use chatmail_db::{get_bool_setting, settings_keys, DbPool};

use crate::cors::CorsSnap;
use crate::response::json_err;

/// Disabled by default unless the DB setting is exactly `"true"`.
pub async fn is_webimap_enabled(pool: &DbPool) -> bool {
    get_bool_setting(pool, settings_keys::WEBIMAP_ENABLED, false)
        .await
        .unwrap_or(false)
}

/// Disabled by default unless the DB setting is exactly `"true"`.
pub async fn is_websmtp_enabled(pool: &DbPool) -> bool {
    get_bool_setting(pool, settings_keys::WEBSMTP_ENABLED, false)
        .await
        .unwrap_or(false)
}

/// Madmail returns HTTP 404 with `{"error":"not found"}` when a service is disabled.
pub fn service_disabled(cors: &CorsSnap) -> Response {
    json_err(StatusCode::NOT_FOUND, "not found", cors)
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::StatusCode;

    use super::*;
    use chatmail_db::{init_memory_db, set_setting, settings_keys};

    #[tokio::test]
    async fn webimap_disabled_by_default() {
        let pool = init_memory_db().await.unwrap();
        assert!(!is_webimap_enabled(&pool).await);
    }

    #[tokio::test]
    async fn webimap_enabled_only_when_setting_true() {
        let pool = init_memory_db().await.unwrap();
        set_setting(&pool, settings_keys::WEBIMAP_ENABLED, "false")
            .await
            .unwrap();
        assert!(!is_webimap_enabled(&pool).await);

        set_setting(&pool, settings_keys::WEBIMAP_ENABLED, "true")
            .await
            .unwrap();
        assert!(is_webimap_enabled(&pool).await);
    }

    #[tokio::test]
    async fn websmtp_enabled_only_when_setting_true() {
        let pool = init_memory_db().await.unwrap();
        set_setting(&pool, settings_keys::WEBSMTP_ENABLED, "true")
            .await
            .unwrap();
        assert!(is_websmtp_enabled(&pool).await);
    }

    #[tokio::test]
    async fn service_disabled_returns_madmail_404_json() {
        let resp = service_disabled(&crate::cors::CorsSnap::empty());
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.as_ref(), br#"{"error":"not found"}"#);
    }
}
