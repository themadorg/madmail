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
use chatmail_auth::{
    hash_password, normalize_username, schedule_hash_upgrade_if_needed, verify_password,
};
use chatmail_config::{build_dclogin_link, DcloginMailSettings};
use chatmail_db::{
    create_sharing_contact, get_bool_setting, get_sharing_contact, normalize_sharing_url,
    passwords, registration_tokens, settings_keys, sharing_slug_exists, validate_slug,
};
use chatmail_delivery::DeliveryContext;
use chatmail_pgp::{enforce_encryption, EnforceOptions};
use chatmail_smtp::protocol::validate_submission_headers;
use chatmail_types::{ChatmailError, MESSAGE_FILE_TOO_BIG};
use rand::Rng;
use serde::Deserialize;
use serde_json::json;

use crate::assets::www_html_exists;
use crate::contact_sharing::is_reserved_slug;
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
        return render_template(st, &lang_path, None, host)
            .await
            .into_response();
    }
    let en_path = format!("docs/en/{name}");
    if www_html_exists(&en_path, st.www_dir.as_deref()) {
        return render_template(st, &en_path, None, host)
            .await
            .into_response();
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
    render_template(st, legacy, None, host)
        .await
        .into_response()
}

pub async fn share_get(State(st): State<WwwState>, headers: HeaderMap) -> impl IntoResponse {
    if st.sharing.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    render_template(&st, "contact_share.html", None, client_host(&headers)).await
}

pub async fn share_post(
    State(st): State<WwwState>,
    headers: HeaderMap,
    axum::Form(form): axum::Form<ShareForm>,
) -> impl IntoResponse {
    let Some(sharing) = st.sharing.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let raw_url = form.url.as_deref().unwrap_or("").trim();
    if raw_url.is_empty() {
        return plain_error(StatusCode::BAD_REQUEST, "Invite URL is required");
    }
    if !raw_url.starts_with("https://i.delta.chat/#") {
        return plain_error(
            StatusCode::BAD_REQUEST,
            "Only Delta Chat web invite links (https://i.delta.chat/#...) are accepted.",
        );
    }

    let url = match normalize_sharing_url(raw_url) {
        Ok(u) => u,
        Err(e) => return plain_error(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    if url.contains(' ') {
        return plain_error(StatusCode::BAD_REQUEST, "Invalid Delta Chat invite URL.");
    }

    let name = form.name.as_deref().unwrap_or("").trim().to_string();
    let slug = match form
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => random_alnum(8),
        Some(s) => {
            if s.len() < 3 {
                return plain_error(
                    StatusCode::BAD_REQUEST,
                    "Path name must be at least 3 characters.",
                );
            }
            if let Err(e) = validate_slug(s) {
                return plain_error(StatusCode::BAD_REQUEST, &e.to_string());
            }
            if is_reserved_slug(s) {
                return plain_error(StatusCode::BAD_REQUEST, "This path name is reserved.");
            }
            s.to_string()
        }
    };

    let pool = match sharing.pool().await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "contact sharing DB unavailable");
            return plain_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create shareable link",
            );
        }
    };

    if sharing_slug_exists(pool, &slug).await.unwrap_or(false) {
        return plain_error(StatusCode::BAD_REQUEST, "This path name is already taken.");
    }

    if let Err(e) = create_sharing_contact(pool, &slug, &url, &name).await {
        tracing::error!(error = %e, slug = %slug, "failed to store contact share");
        return plain_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create shareable link",
        );
    }

    let custom = CustomFields {
        Slug: slug,
        URL: url,
        Name: name,
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

pub async fn new_account_options(
    State(st): State<WwwState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let cors = st.cors_snap(&headers).await;
    crate::response::options_preflight(&cors)
}

pub async fn new_account(
    State(st): State<WwwState>,
    headers: HeaderMap,
    Query(query): Query<NewAccountQuery>,
    body: Result<Json<NewAccountRequest>, axum::extract::rejection::JsonRejection>,
) -> impl IntoResponse {
    let cors = st.cors_snap(&headers).await;
    let mut registration_token = query.token;
    if registration_token.is_empty() {
        if let Ok(Json(req)) = body {
            registration_token = req.token;
        }
    }
    registration_token = registration_token.trim().to_string();

    if !registration_token.is_empty() {
        if let Err(e) =
            registration_tokens::validate_registration_token(&st.pool, &registration_token).await
        {
            return cors_json(
                StatusCode::FORBIDDEN,
                json!({"error": format!("Invalid registration token: {e}")}),
                &cors,
            );
        }
    } else if get_bool_setting(&st.pool, settings_keys::REGISTRATION_TOKEN_REQUIRED, false)
        .await
        .unwrap_or(false)
    {
        return cors_json(
            StatusCode::FORBIDDEN,
            json!({"error": "Registration token is required"}),
            &cors,
        );
    } else if !get_bool_setting(&st.pool, settings_keys::REGISTRATION_OPEN, true)
        .await
        .unwrap_or(false)
    {
        return cors_json(
            StatusCode::FORBIDDEN,
            json!({"error": "Registration is closed"}),
            &cors,
        );
    }

    const MAX_ATTEMPTS: u32 = 5;
    let domain = st
        .config
        .effective_registration_domain(client_host(&headers));
    for _ in 0..MAX_ATTEMPTS {
        let policy = st.config.credential_policy();
        let user = match normalize_username(&format!(
            "{}@{}",
            random_alnum(policy.generated_username_length()),
            domain
        )) {
            Ok(u) => u,
            Err(e) => {
                return cors_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({"error": e.to_string()}),
                    &cors,
                );
            }
        };
        if st.app.auth.is_blocked(&user) {
            continue;
        }
        let password = random_alnum(policy.generated_password_length());
        let hash = match hash_password(&password) {
            Ok(h) => h,
            Err(e) => {
                return cors_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({"error": e.to_string()}),
                    &cors,
                );
            }
        };
        if passwords::create_user(&st.pool, &user, &hash)
            .await
            .is_err()
        {
            continue;
        }
        if st.app.mailbox_store.init_user_dir(&user).await.is_err() {
            let _ = passwords::delete_user(&st.pool, &user).await;
            return cors_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "failed to init mailbox"}),
                &cors,
            );
        }
        if let Err(e) = registration_tokens::ensure_new_account_quota(&st.pool, &user).await {
            let _ = passwords::delete_user(&st.pool, &user).await;
            return cors_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": e.to_string()}),
                &cors,
            );
        }
        if !registration_token.is_empty() {
            if let Err(e) =
                registration_tokens::attach_registration_token(&st.pool, &user, &registration_token)
                    .await
            {
                let _ = passwords::delete_user(&st.pool, &user).await;
                let _ = chatmail_db::db_execute!(
                    &st.pool,
                    "DELETE FROM quotas WHERE username = ?",
                    user
                );
                return cors_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({"error": e.to_string()}),
                    &cors,
                );
            }
        }
        st.app.auth.insert(&user, &hash);
        let mail = dclogin_mail_settings(&st, &headers).await;
        let dclogin_url = build_dclogin_link(&user, &password, &mail);
        return cors_json(
            StatusCode::OK,
            json!({
                "email": user,
                "password": password,
                "dclogin_url": dclogin_url,
            }),
            &cors,
        );
    }
    cors_json(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({"error": "failed to create account"}),
        &cors,
    )
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
    let cors = st.cors_snap(&headers).await;
    if !is_websmtp_enabled(&st.pool).await {
        return service_disabled(&cors);
    }
    let user = match webimap_authenticate(&st.app, &st.pool, &headers, &cors).await {
        Ok(u) => u,
        Err(resp) => return resp,
    };

    req.from = user.clone();
    if req.to.is_empty() {
        return webimap_error(StatusCode::BAD_REQUEST, "missing recipients", &cors);
    }

    match websmtp_deliver(&st, &user, &req.to, &req.body).await {
        Ok(()) => cors_json(StatusCode::OK, json!({ "status": "sent" }), &cors),
        Err(e) => {
            let (status, msg) = web_delivery_error(&e);
            if status == StatusCode::INTERNAL_SERVER_ERROR {
                tracing::error!(error = %msg, "webimap send delivery failed");
            }
            webimap_error(status, &msg, &cors)
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
    app: &chatmail_state::AppState,
    pool: &chatmail_db::DbPool,
    headers: &HeaderMap,
    cors: &crate::cors::CorsSnap,
) -> Result<String, Response> {
    let email = headers
        .get("x-email")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| webimap_error(StatusCode::UNAUTHORIZED, "missing X-Email header", cors))?;
    let password = headers
        .get("x-password")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            webimap_error(StatusCode::UNAUTHORIZED, "missing X-Password header", cors)
        })?;

    let user = normalize_username(email)
        .map_err(|e| webimap_error(StatusCode::BAD_REQUEST, &e.to_string(), cors))?;

    if app.auth.is_blocked(&user) {
        return Err(webimap_error(StatusCode::FORBIDDEN, "user blocked", cors));
    }

    let Some(hash) = app.auth.get_hash(&user) else {
        return Err(webimap_error(
            StatusCode::UNAUTHORIZED,
            "invalid credentials",
            cors,
        ));
    };

    if !verify_password(password, &hash)
        .map_err(|e| webimap_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string(), cors))?
    {
        return Err(webimap_error(
            StatusCode::UNAUTHORIZED,
            "invalid credentials",
            cors,
        ));
    }

    schedule_hash_upgrade_if_needed(
        pool.clone(),
        std::sync::Arc::clone(&app.auth),
        user.clone(),
        password.to_string(),
        hash,
    );

    Ok(user)
}

fn webimap_error(status: StatusCode, message: &str, cors: &crate::cors::CorsSnap) -> Response {
    crate::response::json_err(status, message, cors)
}

fn cors_json(
    status: StatusCode,
    value: serde_json::Value,
    cors: &crate::cors::CorsSnap,
) -> Response {
    crate::response::with_cors((status, Json(value)).into_response(), cors)
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
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
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

/// Mozilla ISPDB autoconfig for Delta Chat / Thunderbird (`dcaccount:` fetch).
pub async fn mail_autoconfig(State(st): State<WwwState>, headers: HeaderMap) -> impl IntoResponse {
    use axum::http::header;
    use chatmail_config::{build_autoconfig_xml, AutoconfigParams};

    let mail = dclogin_mail_settings(&st, &headers).await;
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
    let params = AutoconfigParams::from_mail_settings(&st.mail_domain, &mail, Some(&runtime));
    let xml = build_autoconfig_xml(&params);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/xml; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(xml))
        .unwrap()
        .into_response()
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
    if let Some(contact) = lookup_shared_contact(&st, &path).await {
        return render_template(
            &st,
            "contact_view.html",
            Some(contact),
            client_host(&headers),
        )
        .await
        .into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

fn plain_error(status: StatusCode, message: &str) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(message.to_string()))
        .unwrap_or_else(|_| status.into_response())
}

async fn lookup_shared_contact(st: &WwwState, path: &str) -> Option<CustomFields> {
    if path.contains('.') || path.contains('/') || is_reserved_slug(path) {
        return None;
    }
    let sharing = st.sharing.as_ref()?;
    let pool = sharing.pool().await.ok()?;
    let contact = get_sharing_contact(pool, path).await.ok()??;
    Some(CustomFields {
        Slug: contact.slug,
        URL: contact.url,
        Name: contact.name,
    })
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
        "css" | "js" | "svg" | "png" | "jpg" | "jpeg" | "ico" => Some("public, max-age=86400"),
        _ => None,
    }
}

fn client_host(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::HOST).and_then(|v| v.to_str().ok())
}

async fn dclogin_mail_settings(st: &WwwState, headers: &HeaderMap) -> DcloginMailSettings {
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

    let db_ports = if st
        .context_cache
        .ensure_fresh(&st.pool, &st.config, st.app.mailbox_store.state_dir())
        .await
        .is_ok()
    {
        st.context_cache
            .snapshot()
            .await
            .map(|c| c.db_ports)
            .unwrap_or_default()
    } else {
        chatmail_config::DbMailPorts::default()
    };

    DcloginMailSettings::from_config_with_db_and_runtime(
        &st.config,
        client_host(headers),
        &db_ports,
        Some(&runtime),
    )
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
#[allow(clippy::field_reassign_with_default)]
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
