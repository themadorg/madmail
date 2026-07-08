// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared JSON + CORS helpers for WebIMAP / WebSMTP.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::json;

use crate::cors::{apply_cors, resolve_allow, CorsSnap};

pub fn json_ok<T: Serialize>(status: StatusCode, value: &T, cors: &CorsSnap) -> Response {
    let mut resp = (status, Json(value)).into_response();
    apply_cors(resp.headers_mut(), resolve_allow(cors));
    resp
}

pub fn json_err(status: StatusCode, message: &str, cors: &CorsSnap) -> Response {
    let mut resp = (status, Json(json!({ "error": message }))).into_response();
    apply_cors(resp.headers_mut(), resolve_allow(cors));
    resp
}

pub fn options_preflight(cors: &CorsSnap) -> Response {
    let allow = resolve_allow(cors);
    let status = if allow.is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::FORBIDDEN
    };
    let mut resp = status.into_response();
    apply_cors(resp.headers_mut(), allow);
    resp
}

pub fn with_cors(mut resp: Response, cors: &CorsSnap) -> Response {
    apply_cors(resp.headers_mut(), resolve_allow(cors));
    resp
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::{HeaderValue, StatusCode};
    use serde_json::json;

    use super::*;
    use crate::cors::{CorsAllow, CorsSnap};

    fn snap_with_star() -> CorsSnap {
        CorsSnap {
            allowed: vec!["*".into()],
            request_origin: Some("http://localhost:5173".into()),
        }
    }

    fn cors_origin(resp: &Response) -> &HeaderValue {
        resp.headers()
            .get("Access-Control-Allow-Origin")
            .expect("cors header")
    }

    #[tokio::test]
    async fn json_ok_includes_cors_and_body() {
        let cors = snap_with_star();
        let resp = json_ok(StatusCode::OK, &json!({"ok": true}), &cors);
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(cors_origin(&resp), "*");
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.as_ref(), br#"{"ok":true}"#);
    }

    #[tokio::test]
    async fn json_err_includes_cors_and_error_field() {
        let cors = snap_with_star();
        let resp = json_err(StatusCode::BAD_REQUEST, "bad input", &cors);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(cors_origin(&resp), "*");
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.as_ref(), br#"{"error":"bad input"}"#);
    }

    #[tokio::test]
    async fn options_preflight_is_no_content_with_cors() {
        let cors = snap_with_star();
        let resp = options_preflight(&cors);
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert_eq!(cors_origin(&resp), "*");
        assert!(resp.headers().get("Access-Control-Allow-Methods").is_some());
    }

    #[tokio::test]
    async fn options_preflight_forbidden_without_allowed_origin() {
        let cors = CorsSnap {
            allowed: vec!["http://127.0.0.1:5173".into()],
            request_origin: Some("http://evil.test".into()),
        };
        let resp = options_preflight(&cors);
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(resp.headers().get("Access-Control-Allow-Origin").is_none());
    }

    #[tokio::test]
    async fn json_ok_reflects_allowed_origin() {
        let cors = CorsSnap {
            allowed: vec!["http://127.0.0.1:5173".into()],
            request_origin: Some("http://127.0.0.1:5173".into()),
        };
        let resp = json_ok(StatusCode::OK, &json!({}), &cors);
        assert_eq!(
            cors_origin(&resp),
            HeaderValue::from_static("http://127.0.0.1:5173")
        );
    }
}