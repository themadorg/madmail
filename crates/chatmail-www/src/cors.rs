// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Configurable CORS for WebIMAP / WebSMTP / `POST /new` browser clients (`__WEBMAIL_CORS_ORIGINS__`).

use axum::http::{header, HeaderMap};
use chatmail_db::{get_setting, settings_keys, DbPool};

use crate::gate::{is_webimap_enabled, is_websmtp_enabled};
use crate::WwwState;

/// Parsed CORS policy for one HTTP request.
#[derive(Clone, Debug)]
pub struct CorsSnap {
    pub allowed: Vec<String>,
    pub request_origin: Option<String>,
    /// When WebIMAP and WebSMTP are both enabled, reflect the request `Origin` (no `*`).
    pub auto_reflect_origin: bool,
}

impl CorsSnap {
    pub fn empty() -> Self {
        Self {
            allowed: Vec::new(),
            request_origin: None,
            auto_reflect_origin: false,
        }
    }

    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        if self.auto_reflect_origin && is_valid_browser_origin(origin) {
            return true;
        }
        origin_allowed(origin, &self.allowed)
    }

    pub fn allows_cross_origin(&self) -> bool {
        self.auto_reflect_origin || !self.allowed.is_empty()
    }
}

/// Split stored setting into origin entries (comma or newline separated).
pub fn parse_origins_list(raw: &str) -> Vec<String> {
    raw.split(&[',', '\n', '\r'][..])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

pub async fn load_origins(pool: &DbPool) -> Vec<String> {
    match get_setting(pool, settings_keys::WEBMAIL_CORS_ORIGINS).await {
        Ok(Some(v)) => parse_origins_list(&v),
        _ => Vec::new(),
    }
}

pub fn origin_header(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

impl WwwState {
    pub async fn cors_snap(&self, headers: &HeaderMap) -> CorsSnap {
        let webimap = is_webimap_enabled(&self.pool).await;
        let websmtp = is_websmtp_enabled(&self.pool).await;
        CorsSnap {
            allowed: load_origins(&self.pool).await,
            request_origin: origin_header(headers),
            auto_reflect_origin: webimap && websmtp,
        }
    }
}

/// Browser origins only (`http://` / `https://`); rejects `null` and non-URL values.
pub fn is_valid_browser_origin(origin: &str) -> bool {
    origin.starts_with("http://") || origin.starts_with("https://")
}

pub fn origin_allowed(origin: &str, allowed: &[String]) -> bool {
    if allowed.is_empty() {
        return false;
    }
    if allowed.iter().any(|o| o == "*") {
        return true;
    }
    allowed.iter().any(|o| o == origin)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CorsAllow {
    Any,
    Reflect(String),
}

pub fn resolve_allow(cors: &CorsSnap) -> Option<CorsAllow> {
    let origin = cors.request_origin.as_deref()?;

    if !cors.allowed.is_empty() {
        if cors.allowed.iter().any(|o| o == "*") {
            return Some(CorsAllow::Any);
        }
        if origin_allowed(origin, &cors.allowed) {
            return Some(CorsAllow::Reflect(origin.to_string()));
        }
    }

    if cors.auto_reflect_origin && is_valid_browser_origin(origin) {
        return Some(CorsAllow::Reflect(origin.to_string()));
    }

    None
}

pub fn apply_cors(headers: &mut HeaderMap, allow: Option<CorsAllow>) {
    let Some(allow) = allow else {
        return;
    };
    match allow {
        CorsAllow::Any => {
            headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
        }
        CorsAllow::Reflect(origin) => {
            if let Ok(v) = origin.parse() {
                headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, v);
            }
            headers.insert(header::VARY, "Origin".parse().unwrap());
        }
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        "GET, POST, PUT, DELETE, OPTIONS".parse().unwrap(),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        "Content-Type, X-Email, X-Password".parse().unwrap(),
    );
}

pub fn append_origin(existing: &str, origin: &str) -> String {
    let mut list = parse_origins_list(existing);
    if !list.iter().any(|o| o == origin) {
        list.push(origin.to_string());
    }
    list.join("\n")
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;

    use chatmail_db::{init_memory_db, set_setting, settings_keys};

    use super::*;

    #[tokio::test]
    async fn cors_snap_auto_reflects_when_both_web_services_enabled() {
        let pool = init_memory_db().await.unwrap();
        set_setting(&pool, settings_keys::WEBIMAP_ENABLED, "true")
            .await
            .unwrap();
        set_setting(&pool, settings_keys::WEBSMTP_ENABLED, "true")
            .await
            .unwrap();
        let cfg = chatmail_config::AppConfig::default();
        let dir = tempfile::tempdir().unwrap();
        let app = std::sync::Arc::new(chatmail_state::AppState::with_quota_and_message_limit(
            dir.path(),
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &cfg,
            pool.clone(),
        ));
        let st = WwwState::new(pool, app, cfg, dir.path());
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            "https://admin.madmail.chat".parse().unwrap(),
        );
        let snap = st.cors_snap(&headers).await;
        assert!(snap.auto_reflect_origin);
        assert_eq!(
            resolve_allow(&snap),
            Some(CorsAllow::Reflect("https://admin.madmail.chat".into()))
        );
    }

    #[test]
    fn parse_origins_splits_commas_and_newlines() {
        let v = parse_origins_list("http://a:1\nhttp://b:2, http://c:3");
        assert_eq!(
            v,
            vec![
                "http://a:1".to_string(),
                "http://b:2".to_string(),
                "http://c:3".to_string()
            ]
        );
    }

    #[test]
    fn resolve_reflects_matching_origin() {
        let cors = CorsSnap {
            allowed: vec!["http://127.0.0.1:5173".into()],
            request_origin: Some("http://127.0.0.1:5173".into()),
            auto_reflect_origin: false,
        };
        assert_eq!(
            resolve_allow(&cors),
            Some(CorsAllow::Reflect("http://127.0.0.1:5173".into()))
        );
    }

    #[test]
    fn resolve_denies_unknown_origin_without_auto_reflect() {
        let cors = CorsSnap {
            allowed: vec!["http://127.0.0.1:5173".into()],
            request_origin: Some("http://evil.test".into()),
            auto_reflect_origin: false,
        };
        assert_eq!(resolve_allow(&cors), None);
    }

    #[test]
    fn resolve_reflects_request_origin_when_web_services_enabled() {
        let cors = CorsSnap {
            allowed: vec![],
            request_origin: Some("https://admin.madmail.chat".into()),
            auto_reflect_origin: true,
        };
        assert_eq!(
            resolve_allow(&cors),
            Some(CorsAllow::Reflect("https://admin.madmail.chat".into()))
        );
    }

    #[test]
    fn auto_reflect_rejects_invalid_origin() {
        let cors = CorsSnap {
            allowed: vec![],
            request_origin: Some("not-a-url".into()),
            auto_reflect_origin: true,
        };
        assert_eq!(resolve_allow(&cors), None);
    }

    #[test]
    fn explicit_whitelist_still_wins_over_auto_reflect() {
        let cors = CorsSnap {
            allowed: vec!["http://127.0.0.1:5173".into()],
            request_origin: Some("https://other.app".into()),
            auto_reflect_origin: true,
        };
        assert_eq!(
            resolve_allow(&cors),
            Some(CorsAllow::Reflect("https://other.app".into()))
        );
    }
}
