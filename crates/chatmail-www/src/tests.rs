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

#![allow(clippy::field_reassign_with_default)]

use std::sync::Arc;

use chatmail_config::{parse_maddy_config, AppConfig, RuntimeListeners};
use chatmail_db::{init_memory_db, set_setting, settings_keys};
use chatmail_state::AppState;

use crate::context_cache::WwwContextCache;
use crate::template::{build_context, TemplateEngine};

#[tokio::test]
async fn www_index_renders() {
    let pool = init_memory_db().await.unwrap();
    let _dir = tempfile::tempdir().unwrap();
    let mut cfg = AppConfig::default();
    cfg.imap_listen = Some("0.0.0.0:1143".into());
    cfg.submission_listen = Some("0.0.0.0:2525".into());
    let runtime = RuntimeListeners {
        imap_plain_addr: Some("0.0.0.0:1143".into()),
        imap_tls_addr: None,
        submission_plain_addr: Some("0.0.0.0:2525".into()),
        submission_tls_addr: None,
        smtp_addr: None,
        http_plain_addr: Some("0.0.0.0:8080".into()),
        http_tls_addr: None,
    };
    let dir = tempfile::tempdir().unwrap();
    let cache = WwwContextCache::new();
    let ctx = build_context(
        &pool,
        &cfg,
        None,
        Some("192.168.0.5:8080"),
        Some(&runtime),
        dir.path(),
        &cache,
    )
    .await
    .unwrap();
    assert_eq!(ctx.DcloginImapSecurity, "plain");
    assert_eq!(ctx.ImapPortStartTLS, "1143");
    let engine = TemplateEngine::new();
    let html = engine.render("index.html", &ctx).unwrap();
    assert!(html.contains("Chat Server"));
    assert!(html.contains("main.css"));
}

#[tokio::test]
async fn www_registration_domain_from_host_ip() {
    let pool = init_memory_db().await.unwrap();
    let cfg = AppConfig::default();
    let dir = tempfile::tempdir().unwrap();
    let cache = WwwContextCache::new();
    let ctx = build_context(
        &pool,
        &cfg,
        None,
        Some("127.0.0.1:8080"),
        None,
        dir.path(),
        &cache,
    )
    .await
    .unwrap();
    assert_eq!(ctx.MailDomain, "[127.0.0.1]");
    let html = TemplateEngine::new().render("index.html", &ctx).unwrap();
    assert!(
        html.contains("formatEmail(username, REGISTRATION_DOMAIN)"),
        "JIT registration must use server registration domain"
    );
    assert!(
        html.contains("const REGISTRATION_DOMAIN = \"127.0.0.1\""),
        "registration domain must be bare IP for formatEmail bracketing"
    );
}

#[tokio::test]
async fn www_smtp_ports_from_db() {
    let pool = init_memory_db().await.unwrap();
    set_setting(&pool, settings_keys::SUBMISSION_PORT, "2587")
        .await
        .unwrap();
    let cfg = AppConfig {
        submission_listen: Some("0.0.0.0:2525".into()),
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let cache = WwwContextCache::new();
    let ctx = build_context(&pool, &cfg, None, None, None, dir.path(), &cache)
        .await
        .unwrap();
    assert_eq!(ctx.SmtpPortStartTLS, "2587");
    let html = TemplateEngine::new().render("index.html", &ctx).unwrap();
    assert!(html.contains("2587"));
}

#[tokio::test]
async fn www_info_page_uses_config_and_db() {
    let pool = init_memory_db().await.unwrap();
    let mut cfg = AppConfig::default();
    cfg.default_quota = Some("1G".into());
    cfg.retention = Some("168h".into());
    cfg.mail_domain = Some("info.example".into());
    let dir = tempfile::tempdir().unwrap();
    let cache = WwwContextCache::new();
    let ctx = build_context(&pool, &cfg, None, None, None, dir.path(), &cache)
        .await
        .unwrap();
    assert_eq!(ctx.DefaultQuota, 1024 * 1024 * 1024);
    assert!(ctx
        .MessageRetentionLine
        .as_deref()
        .is_some_and(|s| s.contains("7 days")));
    let engine = TemplateEngine::new();
    let html = engine.render("info.html", &ctx).unwrap();
    assert!(html.contains("1.0 GB") || html.contains("1024"));
    assert!(html.contains("7 days"));
    assert!(!html.contains("after 20 days"));
}

#[tokio::test]
async fn www_language_from_db() {
    let pool = init_memory_db().await.unwrap();
    set_setting(&pool, settings_keys::LANGUAGE, "fa")
        .await
        .unwrap();
    let cfg = AppConfig::default();
    let dir = tempfile::tempdir().unwrap();
    let cache = WwwContextCache::new();
    let ctx = build_context(&pool, &cfg, None, None, None, dir.path(), &cache)
        .await
        .unwrap();
    assert_eq!(ctx.Language, "fa");
    let html = TemplateEngine::new().render("index.html", &ctx).unwrap();
    assert!(html.contains(r#"lang="fa""#));
    assert!(html.contains(r#"dir="rtl""#));
    assert!(html.contains(r#"const PAGE_LANG = "fa""#));
}

#[tokio::test]
async fn www_static_logo() {
    assert!(crate::assets::read_asset("logo.svg").is_some());
}

#[tokio::test]
async fn default_config_www_is_embedded_ram() {
    let pool = init_memory_db().await.unwrap();
    let cfg = AppConfig::default();
    assert!(cfg.www_dir.is_none());
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(AppState::new(dir.path(), pool.clone()));
    let st = crate::WwwState::new(pool, app, cfg.clone(), dir.path());
    assert!(st.uses_embedded_www());
    assert!(!st.uses_external_www());
    assert!(st.templates.is_embedded());
    assert!(!st.templates.is_external());
    assert!(
        st.load_asset("main.css").is_some(),
        "main.css must be preloaded in RAM"
    );
    assert!(
        st.load_asset("logo.svg").is_some(),
        "logo.svg must be served from embedded RAM"
    );
}

/// External `www_dir` must re-read HTML/CSS from disk when files change (`html-export` + `html-serve`).
#[tokio::test]
async fn external_www_live_reload() {
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join("index.html");
    std::fs::write(
        &index,
        "<!DOCTYPE html><html><body>{{ MailDomain }}</body></html>",
    )
    .unwrap();
    std::fs::write(dir.path().join("marker.css"), "body { color: red; }").unwrap();

    let mut cfg = AppConfig::default();
    cfg.www_dir = Some(dir.path().to_path_buf());
    cfg.mail_domain = Some("live.test".into());

    let engine = TemplateEngine::from_config(&cfg);
    assert!(engine.is_external());

    let pool = init_memory_db().await.unwrap();
    let cache = WwwContextCache::new();
    let ctx = build_context(&pool, &cfg, None, None, None, dir.path(), &cache)
        .await
        .unwrap();
    assert!(engine
        .render("index.html", &ctx)
        .unwrap()
        .contains("live.test"));

    std::fs::write(
        &index,
        "<!DOCTYPE html><html><body>updated-html</body></html>",
    )
    .unwrap();
    std::fs::write(dir.path().join("marker.css"), "body { color: blue; }").unwrap();

    assert!(engine
        .render("index.html", &ctx)
        .unwrap()
        .contains("updated-html"));

    let app = Arc::new(AppState::new(dir.path(), pool.clone()));
    let st = crate::WwwState::new(pool, app, cfg, dir.path());
    let css = st.load_asset("marker.css").unwrap();
    assert!(css.windows(4).any(|w| w == b"blue"));
    let css2 = st.load_asset("marker.css").unwrap();
    assert!(css2.windows(4).any(|w| w == b"blue"));
}

/// External `www_dir` accepts Madmail Go template syntax (`{{if .Field}}`).
#[tokio::test]
async fn external_www_go_template_syntax() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("index.html"),
        r#"<!DOCTYPE html><html><body>{{if .RegistrationOpen}}open{{else}}closed{{end}} {{.MailDomain}}</body></html>"#,
    )
    .unwrap();

    let mut cfg = AppConfig::default();
    cfg.www_dir = Some(dir.path().to_path_buf());
    cfg.mail_domain = Some("go.test".into());

    let engine = TemplateEngine::from_config(&cfg);
    let pool = init_memory_db().await.unwrap();
    let cache = WwwContextCache::new();
    let ctx = build_context(&pool, &cfg, None, None, None, dir.path(), &cache)
        .await
        .unwrap();

    let html = engine.render("index.html", &ctx).unwrap();
    assert!(html.contains("go.test"));
    assert!(
        html.contains("open") || html.contains("closed"),
        "expected registration branch, got: {html}"
    );
}

/// Regression for operator custom pages: `{{if not .RegistrationOpen}}` must render,
/// not fail with Minijinja `unexpected '.'` (was index.html:234-class failures).
#[tokio::test]
async fn external_www_go_if_not_registration_open() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("index.html"),
        r#"<!DOCTYPE html><html><body>
{{if not .RegistrationOpen}}
<p id="closed">registration is closed</p>
{{else}}
<p id="open">registration is open</p>
{{end}}
<span>{{.MailDomain | cleanDomain}}</span>
</body></html>"#,
    )
    .unwrap();

    let mut cfg = AppConfig::default();
    cfg.www_dir = Some(dir.path().to_path_buf());
    cfg.mail_domain = Some("example.org".into());

    let engine = TemplateEngine::from_config(&cfg);
    let pool = init_memory_db().await.unwrap();
    // Force registration closed for deterministic branch.
    chatmail_db::set_setting(
        &pool,
        chatmail_db::settings_keys::REGISTRATION_OPEN,
        "false",
    )
    .await
    .unwrap();
    let cache = WwwContextCache::new();
    let ctx = build_context(&pool, &cfg, None, None, None, dir.path(), &cache)
        .await
        .unwrap();
    assert!(
        !ctx.RegistrationOpen,
        "test setup: registration should be closed"
    );

    let html = engine
        .render("index.html", &ctx)
        .expect("if not .RegistrationOpen must convert and render");
    assert!(
        html.contains("registration is closed"),
        "expected closed branch, got: {html}"
    );
    assert!(!html.contains("registration is open"), "got: {html}");
    assert!(html.contains("example.org"), "got: {html}");
    assert!(!html.contains("{{if"), "Go markers leaked: {html}");
    assert!(!html.contains(".RegistrationOpen"), "got: {html}");
}

#[test]
fn www_credential_policy_from_maddy_conf() {
    let content = r#"
chatmail tls://0.0.0.0:443 {
    mail_domain example.org
    username_length 8
    password_length 16
    min_username_length 8
    max_username_length 20
    password_min_length 8
}
"#;
    let cfg = parse_maddy_config(content).unwrap();
    let p = cfg.credential_policy();
    assert_eq!(p.generated_username_length(), 8);
    assert_eq!(p.generated_password_length(), 16);
    assert_eq!(p.min_username_length, 8);
    assert_eq!(p.password_min_length, 8);
}

#[tokio::test]
async fn binary_download_route_serves_current_executable() {
    use axum::body::to_bytes;
    use axum::http::Request;
    use tower::ServiceExt;

    let pool = init_memory_db().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app_state = Arc::new(AppState::new(dir.path(), pool.clone()));
    let app = crate::www_router(crate::WwwState::new(
        pool,
        app_state,
        AppConfig::default(),
        dir.path(),
    ));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/madmail")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/octet-stream")
    );
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let exe = tokio::fs::read(std::env::current_exe().unwrap())
        .await
        .unwrap();
    assert_eq!(body.as_ref(), exe.as_slice());
}

#[tokio::test]
async fn webimap_send_oversize_returns_message_file_too_big() {
    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use chatmail_auth::hash_password;
    use chatmail_config::{effective_max_message_bytes, AppConfig};
    use chatmail_db::{init_memory_db, passwords, set_setting, settings_keys};
    use chatmail_state::AppState;
    use chatmail_types::MESSAGE_FILE_TOO_BIG;

    let pool = init_memory_db().await.unwrap();
    set_setting(&pool, settings_keys::WEBSMTP_ENABLED, "true")
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = AppConfig::default();
    cfg.appendlimit = Some("1M".into());
    let max = effective_max_message_bytes(&cfg);
    let app_state = Arc::new(AppState::with_quota_and_message_limit(
        dir.path(),
        chatmail_config::DEFAULT_QUOTA_BYTES,
        &cfg,
        pool.clone(),
    ));
    set_setting(&pool, chatmail_db::settings_keys::APPENDLIMIT, "1M")
        .await
        .unwrap();
    set_setting(&pool, chatmail_db::settings_keys::MAX_MESSAGE_SIZE, "1M")
        .await
        .unwrap();
    app_state
        .message_size
        .refresh_from_db(&pool, &cfg)
        .await
        .unwrap();
    let hash = hash_password("secret").unwrap();
    passwords::create_user(&pool, "u@x.org", &hash)
        .await
        .unwrap();
    app_state.auth.hydrate(&pool).await.unwrap();

    let app = crate::www_router(crate::WwwState::new(pool, app_state, cfg, dir.path()));

    let body = "x".repeat((max + 1) as usize);
    let payload = serde_json::json!({
        "from": "u@x.org",
        "to": ["u@x.org"],
        "body": body
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webimap/send")
                .header("x-email", "u@x.org")
                .header("x-password", "secret")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        v.get("error").and_then(|e| e.as_str()),
        Some(MESSAGE_FILE_TOO_BIG)
    );
}

#[test]
fn web_delivery_error_maps_message_too_large_to_413() {
    use crate::handlers::web_delivery_error;
    use chatmail_types::{ChatmailError, MESSAGE_FILE_TOO_BIG};

    let (status, msg) = web_delivery_error(&ChatmailError::MessageTooLarge);
    assert_eq!(status, axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(msg, MESSAGE_FILE_TOO_BIG);
}

#[tokio::test]
async fn app_state_check_message_size_enforces_limit() {
    use chatmail_state::AppState;
    use chatmail_types::ChatmailError;

    let pool = init_memory_db().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = AppConfig::default();
    cfg.appendlimit = Some("4K".into());
    let app = AppState::with_quota_and_message_limit(
        dir.path(),
        chatmail_config::DEFAULT_QUOTA_BYTES,
        &cfg,
        pool,
    );
    app.check_message_size(4096).unwrap();
    let err = app.check_message_size(4097).unwrap_err();
    assert!(matches!(err, ChatmailError::MessageTooLarge));
}

#[tokio::test]
async fn www_state_constructs() {
    let pool = init_memory_db().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(AppState::new(dir.path(), pool.clone()));
    let _state = crate::WwwState::new(pool, app, AppConfig::default(), dir.path());
}

#[test]
fn connect_host_for_dclogin_prefers_fallback_over_localhost() {
    let js = crate::assets::read_asset("main.js").expect("main.js");
    let text = String::from_utf8_lossy(&js.data);
    assert!(
        text.contains("fromPage === 'localhost'"),
        "main.js must skip localhost for dclogin ih/sh"
    );
    assert!(
        text.contains("fromPage === '127.0.0.1'"),
        "main.js must skip loopback for dclogin ih/sh"
    );
}

#[tokio::test]
async fn new_account_returns_dclogin_url_with_ssl_hints() {
    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let pool = init_memory_db().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = AppConfig::default();
    cfg.primary_domain = Some("192.0.2.1".into());
    cfg.imap_tls_listen = Some("0.0.0.0:993".into());
    cfg.submission_tls_listen = Some("0.0.0.0:465".into());

    let app_state = Arc::new(AppState::new(dir.path(), pool.clone()));
    app_state.auth.hydrate(&pool).await.unwrap();
    app_state.listener_ports.set_runtime(
        "0.0.0.0:25",
        None,
        Some("0.0.0.0:993".into()),
        None,
        Some("0.0.0.0:465".into()),
        None,
        None,
    );

    let app = crate::www_router(crate::WwwState::new(pool, app_state, cfg, dir.path()));
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/new")
                .header("host", "192.0.2.1")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let email = v.get("email").and_then(|e| e.as_str()).expect("email");
    let _password = v
        .get("password")
        .and_then(|p| p.as_str())
        .expect("password");
    let url = v
        .get("dclogin_url")
        .and_then(|u| u.as_str())
        .expect("dclogin_url");
    assert!(email.contains("@["));
    assert!(url.starts_with("dclogin:"));
    assert!(url.contains("ih=192.0.2.1"));
    assert!(url.contains("sh=192.0.2.1"));
    assert!(url.contains("is=ssl"));
    assert!(url.contains("ss=ssl"));
    assert!(url.contains(email));
}

#[tokio::test]
async fn mail_autoconfig_omits_https_alpn_entry() {
    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let pool = init_memory_db().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = AppConfig::default();
    cfg.mail_domain = Some("example.org".into());
    cfg.imap_tls_listen = Some("0.0.0.0:993".into());
    cfg.submission_tls_listen = Some("0.0.0.0:465".into());
    cfg.http_tls_listen = Some("0.0.0.0:443".into());

    let app_state = Arc::new(AppState::new(dir.path(), pool.clone()));
    app_state.listener_ports.set_runtime(
        "0.0.0.0:25",
        Some("0.0.0.0:143".into()),
        Some("0.0.0.0:993".into()),
        Some("0.0.0.0:587".into()),
        Some("0.0.0.0:465".into()),
        None,
        Some("0.0.0.0:443".into()),
    );

    let app = crate::www_router(crate::WwwState::new(pool, app_state, cfg, dir.path()));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/.well-known/autoconfig/mail/config-v1.1.xml")
                .header("host", "example.org")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let xml = String::from_utf8_lossy(&bytes);
    assert!(xml.contains("<port>993</port>"));
    assert!(xml.contains("<port>143</port>"));
    assert!(!xml.contains("<port>443</port>"));
}

/// Contact sharing: POST /share persists to sharing.db; GET /{slug} renders contact page.
#[tokio::test]
async fn contact_sharing_post_and_slug_view() {
    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use chatmail_db::{get_sharing_contact, init_sharing_db};
    use chatmail_state::AppState;

    let pool = init_memory_db().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = AppConfig::default();
    cfg.enable_contact_sharing = true;
    cfg.mail_domain = Some("share.test".into());

    let app_state = Arc::new(AppState::new(dir.path(), pool.clone()));
    let app = crate::www_router(crate::WwwState::new(
        pool.clone(),
        app_state,
        cfg,
        dir.path(),
    ));

    let form = "url=https%3A%2F%2Fi.delta.chat%2F%23ABCDEF0123456789%26name%3DTest&name=Alice&slug=alicepage";
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/share")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(axum::body::Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("alicepage"));

    let sharing_db = dir.path().join("sharing.db");
    let sharing_pool = init_sharing_db(&sharing_db).await.unwrap();
    let row = get_sharing_contact(&sharing_pool, "alicepage")
        .await
        .unwrap()
        .expect("contact row");
    assert_eq!(row.name, "Alice");
    assert!(row.url.starts_with("openpgp4fpr:"));

    let view = app
        .oneshot(
            Request::builder()
                .uri("/alicepage")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(view.status(), StatusCode::OK);
    let view_body = to_bytes(view.into_body(), usize::MAX).await.unwrap();
    let page = String::from_utf8_lossy(&view_body);
    assert!(page.contains("Alice"));
    assert!(page.contains("openpgp4fpr:"));
}
