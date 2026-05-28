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

use std::sync::Arc;

use axum::extract::{Extension, Request};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use chatmail_db::settings_keys::ADMIN_WEB_ENABLED;
use chatmail_db::{get_bool_setting, DbPool};
use rust_embed::EmbeddedFile;

use crate::assets::AdminWebAssets;
use crate::patch::patch_index_html;

const UNAVAILABLE_HTML: &str = r#"<!doctype html><html><body><h1>Admin Web UI Not Available</h1>
<p>The admin web dashboard was not included in this build. Build <code>external/madmail-admin-web</code> first, then rebuild chatmail.</p></body></html>"#;

#[derive(Clone)]
pub struct AdminWebState {
    pub pool: DbPool,
    pub prefix: String,
    pub patched_index: Arc<Vec<u8>>,
    pub build_available: bool,
}

impl AdminWebState {
    pub fn new(pool: DbPool, prefix: String) -> Self {
        let raw_index = AdminWebAssets::get("index.html");
        let build_available = raw_index
            .as_ref()
            .map(|f| f.data.len() > 200)
            .unwrap_or(false);

        let patched_index = if let Some(file) = raw_index {
            Arc::new(patch_index_html(
                std::str::from_utf8(file.data.as_ref()).unwrap_or(""),
                &prefix,
            ))
        } else {
            Arc::new(Vec::new())
        };

        Self {
            pool,
            prefix,
            patched_index,
            build_available,
        }
    }

    async fn enabled(&self) -> bool {
        get_bool_setting(&self.pool, ADMIN_WEB_ENABLED, false)
            .await
            .unwrap_or(false)
    }
}

/// HTTP routes for the embedded admin-web SPA under `prefix` (e.g. `/admin` or `/xxx`).
///
/// Uses explicit paths (not `nest`) so `/admin/…` wins over www `/{*path}` when routers are
/// merged. Uses [`Extension`] instead of [`axum::extract::State`] for cross-router merges.
pub fn admin_web_router(state: AdminWebState) -> Router {
    let prefix = state.prefix.trim_end_matches('/').to_string();
    let redirect_target = format!("{prefix}/");
    let shared = Arc::new(state);

    Router::new()
        .route(
            &prefix,
            get({
                let redirect_target = redirect_target.clone();
                move || async move { Redirect::permanent(&redirect_target) }
            }),
        )
        .route(
            &format!("{prefix}/"),
            axum::routing::any(spa_fallback),
        )
        .route(
            &format!("{prefix}/{{*filepath}}"),
            axum::routing::any(spa_fallback),
        )
        .layer(Extension(shared))
}

async fn spa_fallback(Extension(st): Extension<Arc<AdminWebState>>, req: Request) -> Response {
    if req.method() == Method::OPTIONS {
        return options_response();
    }
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if !st.enabled().await {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !st.build_available {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Html(UNAVAILABLE_HTML),
        )
            .into_response();
    }

    let path = req
        .uri()
        .path()
        .strip_prefix(st.prefix.as_str())
        .unwrap_or(req.uri().path())
        .trim_start_matches('/');

    if path.is_empty() {
        return serve_index(st.as_ref());
    }

    match AdminWebAssets::get(path) {
        Some(file) => serve_asset(path, file),
        None => serve_index(st.as_ref()),
    }
}

fn serve_index(st: &AdminWebState) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/html; charset=utf-8".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    (StatusCode::OK, headers, st.patched_index.as_ref().clone()).into_response()
}

fn serve_asset(path: &str, file: EmbeddedFile) -> Response {
    let mut headers = HeaderMap::new();
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    headers.insert(header::CONTENT_TYPE, mime.parse().unwrap());

    let cache = if path.contains("/immutable/") {
        "public, max-age=31536000, immutable"
    } else if path == "version.json" || path == "sw.js" {
        "no-cache, no-store, must-revalidate"
    } else {
        "public, max-age=3600"
    };
    headers.insert(header::CACHE_CONTROL, cache.parse().unwrap());

    (StatusCode::OK, headers, file.data.to_vec()).into_response()
}

fn options_response() -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        "GET, HEAD, OPTIONS".parse().unwrap(),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        "Content-Type".parse().unwrap(),
    );
    (StatusCode::NO_CONTENT, headers).into_response()
}
