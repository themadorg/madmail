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

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Json;
use chatmail_auth::{hash_password, normalize_username, verify_password};
use chatmail_db::{
    blocklist, get_bool_setting, passwords, registration_tokens, settings_keys,
};
use chatmail_delivery::DeliveryContext;
use chatmail_pgp::{enforce_encryption, EnforceOptions};
use chatmail_smtp::protocol::validate_submission_headers;
use chatmail_types::{ChatmailError, MESSAGE_FILE_TOO_BIG};
use rand::Rng;
use serde::Deserialize;
use serde_json::json;

use crate::assets::www_html_exists;
use crate::gate::{is_websmtp_enabled, service_disabled};
use crate::template::{build_context, CustomFields};
use crate::WwwState;

#[derive(Deserialize)]
pub struct ShareForm {
    pub url: Option<String>,
    pub name: Option<String>,
    pub slug: Option<String>,
}

pub async fn index(State(st): State<WwwState>, headers: HeaderMap) -> impl IntoResponse {
    render_template(&st, "index.html", None, client_host(&headers)).await
}

pub async fn template_page(
    State(st): State<WwwState>,
    headers: HeaderMap,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> impl IntoResponse {
    if !name.ends_with(".html") {
        return StatusCode::NOT_FOUND.into_response();
    }
    render_template(&st, &name, None, client_host(&headers)).await
}

pub async fn docs_redirect() -> impl IntoResponse {
    Redirect::permanent("/docs/")
}

pub async fn docs_index(State(st): State<WwwState>, headers: HeaderMap) -> impl IntoResponse {
    render_template(&st, "docs_index.html", None, client_host(&headers)).await
}

pub async fn docs_path(
    State(st): State<WwwState>,
    headers: HeaderMap,
    axum::extract::Path(sub): axum::extract::Path<String>,
) -> impl IntoResponse {
    let sub = sub.trim_matches('/');
    let file = match sub {
        "" | "index" | "index.html" => {
            return docs_index(State(st), headers).await.into_response();
        }
        "admin" => doc_lang(&st, "admin.html", &headers).await,
        "api" => render_template(&st, "admin_api_docs.html", None, client_host(&headers)).await,
        "general" => doc_lang(&st, "general.html", &headers).await,
        "serve" | "custom-html" => doc_lang(&st, "serve.html", &headers).await,
        "database" => doc_lang(&st, "database.html", &headers).await,
        "docker" => doc_lang(&st, "docker.html", &headers).await,
        "relay" | "domain" | "tls" => doc_lang(&st, "relay.html", &headers).await,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    file.into_response()
}

async fn doc_lang(st: &WwwState, name: &str, headers: &HeaderMap) -> Response {
    let host = client_host(headers);
    let state_dir = st.app.mailbox_store.state_dir();
    let lang = if st
        .context_cache
        .ensure_fresh(&st.pool, &st.config, state_dir)
        .await
        .is_ok()
    {
        st.context_cache
            .snapshot()
            .await
            .map(|s| s.language)
            .unwrap_or_else(|| "en".into())
    } else {
        "en".into()
    };
    let lang_path = format!("docs/{lang}/{name}");
    if www_html_exists(&lang_path, st.www_dir.as_deref()) {
        return render_template(st, &lang_path, None, host).await.into_response();
    }
    let en_path = format!("docs/en/{name}");
    if www_html_exists(&en_path, st.www_dir.as_deref()) {
        return render_template(st, &en_path, None, host).await.into_response();
    }
    let legacy = match name {
        "general.html" => "general_docs.html",
        "serve.html" => "docs_serve.html",
        "database.html" => "database_docs.html",
        "docker.html" => "docker_docs.html",
        "relay.html" => "relay_docs.html",
        "admin.html" => "admin_docs.html",
        _ => name,
    };
    render_template(st, legacy, None, host).await.into_response()
}

pub async fn share_get(State(st): State<WwwState>, headers: HeaderMap) -> impl IntoResponse {
    render_template(&st, "contact_share.html", None, client_host(&headers)).await
}

pub async fn share_post(
    State(st): State<WwwState>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<ShareForm>,
) -> impl IntoResponse {
    let slug = form
        .slug
        .filter(|s| s.len() >= 3)
        .unwrap_or_else(|| random_alnum(8));
    let custom = CustomFields {
        Slug: slug,
        URL: form.url.unwrap_or_default(),
        Name: form.name.unwrap_or_default(),
    };
    render_template(
        &st,
        "contact_share_success.html",
        Some(custom),
        client_host(&headers),
    )
    .await
}

pub async fn app_page(State(st): State<WwwState>, headers: HeaderMap) -> impl IntoResponse {
    render_template(&st, "app.html", None, client_host(&headers)).await
}

pub async fn invite_page(State(st): State<WwwState>, headers: HeaderMap) -> impl IntoResponse {
    render_template(&st, "invite.html", None, client_host(&headers)).await
}

#[derive(Deserialize, Default)]
pub struct NewAccountRequest {
    #[serde(default)]
    pub token: String,
}

#[derive(Deserialize, Default)]
pub struct NewAccountQuery {
    #[serde(default)]
    pub token: String,
}

pub async fn new_account(
    State(st): State<WwwState>,
    headers: HeaderMap,
    Query(query): Query<NewAccountQuery>,
    body: Result<Json<NewAccountRequest>, axum::extract::rejection::JsonRejection>,
) -> impl IntoResponse {
    let mut registration_token = query.token;
    if registration_token.is_empty() {
        if let Ok(Json(req)) = body {
            registration_token = req.token;
        }
    }
    registration_token = registration_token.trim().to_string();

    if !registration_token.is_empty() {
        if let Err(e) = registration_tokens::validate_registration_token(
            &st.pool,
            &registration_token,
        )
        .await
        {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": format!("Invalid registration token: {e}")})),
            )
                .into_response();
        }
    } else if get_bool_setting(
        &st.pool,
        settings_keys::REGISTRATION_TOKEN_REQUIRED,
        false,
    )
    .await
    .unwrap_or(false)
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Registration token is required"})),
        )
            .into_response();
    } else if !get_bool_setting(&st.pool, settings_keys::REGISTRATION_OPEN, true)
        .await
        .unwrap_or(false)
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Registration is closed"})),
        )
            .into_response();
    }

    const MAX_ATTEMPTS: u32 = 5;
    let domain = st
        .config
        .effective_registration_domain(client_host(&headers).as_deref());
    for _ in 0..MAX_ATTEMPTS {
        let policy = st.config.credential_policy();
        let user = match normalize_username(&format!(
            "{}@{}",
            random_alnum(policy.generated_username_length()),
            domain
        )) {
            Ok(u) => u,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string()})),
                )
                    .into_response();
            }
        };
        if blocklist::is_blocked(&st.pool, &user).await.unwrap_or(false) {
            continue;
        }
        let password = random_alnum(policy.generated_password_length());
        let hash = match hash_password(&password) {
            Ok(h) => h,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string()})),
                )
                    .into_response();
            }
        };
        if passwords::create_user(&st.pool, &user, &hash).await.is_err() {
            continue;
        }
        if st
            .app
            .mailbox_store
            .init_user_dir(&user)
            .await
            .is_err()
        {
            let _ = passwords::delete_user(&st.pool, &user).await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        if let Err(e) = registration_tokens::ensure_new_account_quota(&st.pool, &user).await {
            let _ = passwords::delete_user(&st.pool, &user).await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response();
        }
        if !registration_token.is_empty() {
            if let Err(e) = registration_tokens::attach_registration_token(
                &st.pool,
                &user,
                &registration_token,
            )
            .await
            {
                let _ = passwords::delete_user(&st.pool, &user).await;
                let _ = chatmail_db::db_execute!(
                    &st.pool,
                    "DELETE FROM quotas WHERE username = ?",
                    user
                );
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string()})),
                )
                    .into_response();
            }
        }
        return Json(json!({ "email": user, "password": password })).into_response();
    }
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

#[derive(Deserialize)]
pub struct WebimapSendRequest {
    pub from: String,
    pub to: Vec<String>,
    pub body: String,
}

/// POST `/webimap/send` or `/websmtp/send` — WebSMTP (Madmail `websmtp.go`).
pub async fn webimap_send(
    State(st): State<WwwState>,
    headers: HeaderMap,
    Json(mut req): Json<WebimapSendRequest>,
) -> impl IntoResponse {
    if !is_websmtp_enabled(&st.pool).await {
        return service_disabled();
    }
    let user = match webimap_authenticate(&st.pool, &headers).await {
        Ok(u) => u,
        Err(resp) => return resp,
    };

    req.from = user.clone();
    if req.to.is_empty() {
        return webimap_error(
            StatusCode::BAD_REQUEST,
            "missing recipients",
        );
    }

    match websmtp_deliver(&st, &user, &req.to, &req.body).await {
        Ok(()) => Json(json!({ "status": "sent" })).into_response(),
        Err(e) => {
            let (status, msg) = web_delivery_error(&e);
            if status == StatusCode::INTERNAL_SERVER_ERROR {
                tracing::error!(error = %msg, "webimap send delivery failed");
            }
            webimap_error(status, &msg)
        }
    }
}

pub(crate) fn web_delivery_error(e: &ChatmailError) -> (StatusCode, String) {
    match e {
        ChatmailError::MessageTooLarge => {
            (StatusCode::PAYLOAD_TOO_LARGE, MESSAGE_FILE_TOO_BIG.to_string())
        }
        ChatmailError::EncryptionNeeded(m) => (
            StatusCode::BAD_REQUEST,
            format!(
                "Encryption Needed: only PGP-encrypted messages and SecureJoin handshakes are accepted: {m}"
            ),
        ),
        ChatmailError::QuotaExceeded { .. } => {
            (StatusCode::PAYLOAD_TOO_LARGE, "552 5.2.2 Quota exceeded".into())
        }
        ChatmailError::FederationRejected(d) => (
            StatusCode::BAD_REQUEST,
            format!("federation rejected: {d}"),
        ),
        ChatmailError::Protocol(m) | ChatmailError::Config(m) | ChatmailError::Storage(m) => {
            (StatusCode::BAD_REQUEST, m.clone())
        }
        ChatmailError::UserBlocked(u) => (StatusCode::FORBIDDEN, format!("user blocked: {u}")),
        ChatmailError::AuthFailed => (
            StatusCode::UNAUTHORIZED,
            "authentication failed".into(),
        ),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

/// Shared WebSMTP delivery for REST and WebSocket `send`.
pub async fn websmtp_deliver(
    st: &WwwState,
    user: &str,
    to: &[String],
    body: &str,
) -> Result<(), ChatmailError> {
    let raw = body.as_bytes();
    st.app.check_message_size(raw.len())?;
    validate_submission_headers(raw, user)?;

    enforce_encryption(
        raw,
        &EnforceOptions {
            mail_from: user.to_string(),
            recipients: to.to_vec(),
        },
    )?;

    let primary = st
        .config
        .primary_domain
        .clone()
        .unwrap_or_else(|| st.mail_domain.clone());
    let delivery = DeliveryContext {
        pool: st.pool.clone(),
        state: Arc::clone(&st.app),
        primary_domain: primary,
        local_domains: st.local_domains.clone(),
    };

    delivery.route_message(user, to, raw).await
}

pub(crate) async fn webimap_authenticate(
    pool: &chatmail_db::DbPool,
    headers: &HeaderMap,
) -> Result<String, Response> {
    let email = headers
        .get("x-email")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| webimap_error(StatusCode::UNAUTHORIZED, "missing X-Email header"))?;
    let password = headers
        .get("x-password")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| webimap_error(StatusCode::UNAUTHORIZED, "missing X-Password header"))?;

    let user = normalize_username(email)
        .map_err(|e| webimap_error(StatusCode::BAD_REQUEST, &e.to_string()))?;

    if blocklist::is_blocked(pool, &user).await.unwrap_or(false) {
        return Err(webimap_error(StatusCode::FORBIDDEN, "user blocked"));
    }

    let Some(hash) = passwords::get_user_hash(pool, &user)
        .await
        .map_err(|e| webimap_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
    else {
        return Err(webimap_error(StatusCode::UNAUTHORIZED, "invalid credentials"));
    };

    if !verify_password(password, &hash)
        .map_err(|e| webimap_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
    {
        return Err(webimap_error(StatusCode::UNAUTHORIZED, "invalid credentials"));
    }

    Ok(user)
}

fn webimap_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// Serve the running executable at `GET /madmail` (Madmail `handleBinaryDownload`).
pub async fn binary_download(method: Method) -> impl IntoResponse {
    if method != Method::GET {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }

    let path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "binary download: current_exe");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("madmail");

    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, path = %path.display(), "binary download: read");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let disposition = format!("attachment; filename={filename}");
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    if let Ok(v) = HeaderValue::try_from(disposition) {
        headers.insert(header::CONTENT_DISPOSITION, v);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(header::EXPIRES, HeaderValue::from_static("0"));

    (headers, bytes).into_response()
}

pub async fn catch_all(
    State(st): State<WwwState>,
    headers: HeaderMap,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl IntoResponse {
    if path.is_empty() {
        return index(State(st), headers).await.into_response();
    }
    if path.ends_with(".html") {
        return template_page(State(st), headers, axum::extract::Path(path))
            .await
            .into_response();
    }
    if let Some(resp) = serve_bytes(&st, &path) {
        return resp.into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

fn serve_bytes(st: &WwwState, path: &str) -> Option<Response> {
    let data = st.load_asset(path)?;
    let mime = static_mime(path)?;
    let cache_control = static_cache_control(path, st.uses_external_www());
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime);
    if let Some(cc) = cache_control {
        builder = builder.header(header::CACHE_CONTROL, cc);
    }
    builder.body(Body::from(data.to_vec())).ok()
}

fn static_mime(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next()? {
        "css" => Some("text/css"),
        "js" => Some("application/javascript"),
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "ico" => Some("image/x-icon"),
        _ => Some("application/octet-stream"),
    }
}

/// Browser cache: long-lived for embedded www; disabled for external `www_dir` (dev/edit loop).
fn static_cache_control(path: &str, live_www_dir: bool) -> Option<&'static str> {
    if live_www_dir {
        return Some("no-cache, must-revalidate");
    }
    match path.rsplit('.').next()? {
        "css" | "js" | "svg" | "png" | "jpg" | "jpeg" | "ico" => {
            Some("public, max-age=86400")
        }
        _ => None,
    }
}

fn client_host(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
}

async fn render_template(
    st: &WwwState,
    name: &str,
    custom: Option<CustomFields>,
    http_host: Option<&str>,
) -> Response {
    let snap = st.app.listener_ports.snapshot();
    let runtime = chatmail_config::RuntimeListeners {
        imap_plain_addr: snap.imap_plain_addr,
        imap_tls_addr: snap.imap_tls_addr,
        submission_plain_addr: snap.submission_plain_addr,
        submission_tls_addr: snap.submission_tls_addr,
        smtp_addr: snap.smtp_addr,
        http_plain_addr: snap.http_plain_addr,
        http_tls_addr: snap.http_tls_addr,
    };
    let ctx = match build_context(
        &st.pool,
        &st.config,
        custom,
        http_host,
        Some(&runtime),
        st.app.mailbox_store.state_dir(),
        &st.context_cache,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(%e, file = %name, "www template context");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    match st.templates.render(name, &ctx) {
        Ok(html) => {
            let mut resp = Html(html).into_response();
            if st.uses_external_www() {
                resp.headers_mut().insert(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("no-cache, must-revalidate"),
                );
            }
            resp
        }
        Err(e) => {
            tracing::error!(%e, file = %name, "www template render");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn random_alnum(len: usize) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..len)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod random_alnum_tests {
    use super::random_alnum;
    use chatmail_config::AppConfig;

    #[test]
    fn random_alnum_exact_length() {
        assert_eq!(random_alnum(8).len(), 8);
        assert_eq!(random_alnum(16).len(), 16);
        assert!(random_alnum(8).chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn policy_generated_lengths_match_config() {
        let mut cfg = AppConfig::default();
        cfg.username_length = Some(8);
        cfg.password_length = Some(16);
        cfg.min_username_length = Some(8);
        cfg.max_username_length = Some(20);
        let p = cfg.credential_policy();
        assert_eq!(random_alnum(p.generated_username_length()).len(), 8);
        assert_eq!(random_alnum(p.generated_password_length()).len(), 16);
    }
}
