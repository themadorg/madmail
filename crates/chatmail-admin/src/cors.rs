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

//! CORS for the JSON admin API (hosted admin UI at `https://admin.madmail.chat`).

use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;

/// Official hosted Madmail admin panel (browser origin).
pub const MADMAIL_ADMIN_PANEL_ORIGIN: &str = "https://admin.madmail.chat";

const DEFAULT_ORIGINS: &[&str] = &[
    MADMAIL_ADMIN_PANEL_ORIGIN,
    "http://admin.madmail.chat",
];

/// Comma-separated extra origins (`CHATMAIL_ADMIN_CORS_ORIGINS`).
fn configured_origins() -> Vec<String> {
    let mut out: Vec<String> = DEFAULT_ORIGINS.iter().map(|s| (*s).to_string()).collect();
    if let Ok(extra) = std::env::var("CHATMAIL_ADMIN_CORS_ORIGINS") {
        for o in extra.split(',') {
            let o = o.trim();
            if !o.is_empty() && !out.iter().any(|x| x == o) {
                out.push(o.to_string());
            }
        }
    }
    out
}

pub fn is_allowed_origin(origin: &str) -> bool {
    configured_origins().iter().any(|allowed| allowed == origin)
}

fn allow_origin_value(origin: &str) -> Option<HeaderValue> {
    if is_allowed_origin(origin) {
        origin.parse().ok()
    } else {
        None
    }
}

pub fn apply_cors_headers(origin: &str, headers: &mut HeaderMap) {
    let Some(value) = allow_origin_value(origin) else {
        return;
    };
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Authorization, Content-Type"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("86400"),
    );
}

pub fn preflight_response(origin: &str) -> Response<Body> {
    let mut headers = HeaderMap::new();
    apply_cors_headers(origin, &mut headers);
    (StatusCode::NO_CONTENT, headers).into_response()
}

/// Handle browser preflight and attach CORS headers to successful admin responses.
pub async fn cors_middleware(request: Request<Body>, next: Next) -> Response<Body> {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    if request.method() == Method::OPTIONS {
        if let Some(ref o) = origin {
            if is_allowed_origin(o) {
                return preflight_response(o);
            }
        }
        return StatusCode::FORBIDDEN.into_response();
    }

    let mut response = next.run(request).await;
    if let Some(ref o) = origin {
        apply_cors_headers(o, response.headers_mut());
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_official_admin_panel() {
        assert!(is_allowed_origin(MADMAIL_ADMIN_PANEL_ORIGIN));
        assert!(!is_allowed_origin("https://evil.example"));
    }

    #[test]
    fn preflight_sets_allow_origin() {
        let res = preflight_response(MADMAIL_ADMIN_PANEL_ORIGIN);
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            res.headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|v| v.to_str().ok()),
            Some(MADMAIL_ADMIN_PANEL_ORIGIN)
        );
    }
}
