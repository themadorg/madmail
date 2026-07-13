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

//! Madmail-compatible WebIMAP REST + WebSocket for the `/app` web UI.

use std::time::Duration;

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chatmail_storage::{
    copy_message, delete_blob, list_inbox, list_mailbox_messages, mailbox_exists, move_message,
    read_blob, store_add_flags, InboxEntry,
};
use mail_parser::MessageParser;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cors::CorsSnap;
use crate::gate::{is_webimap_enabled, service_disabled};
use crate::handlers::webimap_authenticate;
use crate::response::{json_err, json_ok, options_preflight as cors_options_preflight};
use crate::WwwState;

#[derive(Serialize)]
pub(crate) struct MailboxInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<String>,
    pub messages: u32,
    pub unseen: u32,
}

#[derive(Serialize)]
struct Address {
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    mailbox: String,
    host: String,
}

#[derive(Serialize)]
struct Envelope {
    date: String,
    subject: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    from: Vec<Address>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    to: Vec<Address>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cc: Vec<Address>,
    #[serde(skip_serializing_if = "String::is_empty")]
    message_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    in_reply_to: String,
}

#[derive(Serialize)]
pub(crate) struct MessageSummary {
    pub uid: u32,
    pub seq_num: u32,
    flags: Vec<String>,
    size: u32,
    date: String,
    envelope: Envelope,
}

#[derive(Serialize)]
pub(crate) struct MessageDetail {
    #[serde(flatten)]
    summary: MessageSummary,
    body: String,
}

#[derive(Deserialize)]
pub struct MessagesQuery {
    pub mailbox: Option<String>,
    pub since_uid: Option<u32>,
    pub wait: Option<u32>,
}

#[derive(Deserialize)]
pub struct MessagePath {
    pub uid: u32,
}

#[derive(Deserialize)]
pub struct MessageQuery {
    pub mailbox: Option<String>,
}

#[derive(Deserialize)]
pub struct MessagesDeletePath {
    pub mailbox: String,
    pub uid: u32,
}

#[derive(Deserialize)]
pub struct WsQuery {
    pub email: String,
    pub password: String,
    pub mailbox: Option<String>,
    pub since_uid: Option<u32>,
}

fn parse_envelope(raw: &[u8]) -> (Envelope, String) {
    let body = String::from_utf8_lossy(raw).into_owned();
    let mut env = Envelope {
        date: String::new(),
        subject: String::new(),
        from: Vec::new(),
        to: Vec::new(),
        cc: Vec::new(),
        message_id: String::new(),
        in_reply_to: String::new(),
    };
    let Some(msg) = MessageParser::default().parse(raw) else {
        return (env, body);
    };
    if let Some(d) = msg.date() {
        env.date = d.to_rfc3339();
    }
    env.subject = msg.subject().unwrap_or_default().to_string();
    env.message_id = msg.message_id().unwrap_or_default().to_string();
    env.in_reply_to = msg.in_reply_to().as_text().unwrap_or_default().to_string();
    env.from = convert_addrs(msg.from());
    env.to = convert_addrs(msg.to());
    env.cc = convert_addrs(msg.cc());
    (env, body)
}

fn convert_addrs(addrs: Option<&mail_parser::Address<'_>>) -> Vec<Address> {
    let Some(addrs) = addrs else {
        return Vec::new();
    };
    addrs
        .iter()
        .filter_map(|a| {
            let email = a.address.as_ref()?;
            let (mailbox, host) = email.split_once('@')?;
            Some(Address {
                name: a.name.as_ref().map(|n| n.to_string()).unwrap_or_default(),
                mailbox: mailbox.to_string(),
                host: host.to_string(),
            })
        })
        .collect()
}

pub(crate) fn entry_to_summary(entry: &InboxEntry, raw: &[u8]) -> MessageSummary {
    let (envelope, _) = parse_envelope(raw);
    MessageSummary {
        uid: entry.uid,
        seq_num: entry.uid,
        flags: vec!["\\Seen".into()],
        size: entry.size.min(u32::MAX as u64) as u32,
        date: if envelope.date.is_empty() {
            chrono_lite_now()
        } else {
            envelope.date.clone()
        },
        envelope,
    }
}

fn chrono_lite_now() -> String {
    // RFC3339 without pulling chrono into www
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

pub(crate) async fn load_mailbox_entries(
    st: &WwwState,
    user: &str,
    mailbox: &str,
) -> Result<Vec<InboxEntry>, String> {
    if !user_mailbox_exists(st, user, mailbox).await {
        return Err("unknown mailbox".into());
    }
    if mailbox.eq_ignore_ascii_case("INBOX") {
        return list_inbox(&st.app.mailbox_store, user)
            .await
            .map_err(|e| e.to_string());
    }
    let msgs = list_mailbox_messages(&st.app.mailbox_store, user, mailbox)
        .await
        .map_err(|e| e.to_string())?;
    Ok(msgs
        .into_iter()
        .map(|m| InboxEntry {
            uid: m.uid,
            msg_id: m.base_id,
            size: m.size,
        })
        .collect())
}

pub(crate) async fn user_mailbox_exists(st: &WwwState, user: &str, mailbox: &str) -> bool {
    if mailbox.eq_ignore_ascii_case("INBOX") {
        return true;
    }
    mailbox_exists(&st.app.mailbox_store, user, mailbox).await
}

pub(crate) async fn list_user_mailboxes(
    st: &WwwState,
    user: &str,
) -> Result<Vec<MailboxInfo>, String> {
    let mut names = vec!["INBOX".to_string()];
    let folders = st
        .app
        .mailbox_store
        .maildir_for_user(user)
        .root
        .parent()
        .map(|p| p.join("folders"));
    if let Some(dir) = folders {
        if dir.is_dir() {
            let mut rd = tokio::fs::read_dir(&dir).await.map_err(|e| e.to_string())?;
            while let Some(ent) = rd.next_entry().await.map_err(|e| e.to_string())? {
                if ent.file_type().await.map_err(|e| e.to_string())?.is_dir() {
                    names.push(ent.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    let mut out = Vec::new();
    for name in names {
        let entries = load_mailbox_entries(st, user, &name).await?;
        let count = entries.len() as u32;
        out.push(MailboxInfo {
            name,
            attributes: vec![],
            messages: count,
            unseen: count,
        });
    }
    Ok(out)
}

pub(crate) async fn find_entry(entries: &[InboxEntry], uid: u32) -> Option<InboxEntry> {
    entries.iter().find(|e| e.uid == uid).cloned()
}

pub(crate) async fn build_detail(
    st: &WwwState,
    user: &str,
    mailbox: &str,
    entry: &InboxEntry,
    cors: &CorsSnap,
) -> Result<MessageDetail, Response> {
    let raw = read_blob(&st.app.mailbox_store, user, mailbox, &entry.msg_id)
        .await
        .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string(), cors))?;
    let (envelope, body) = parse_envelope(&raw);
    let date = if envelope.date.is_empty() {
        chrono_lite_now()
    } else {
        envelope.date.clone()
    };
    Ok(MessageDetail {
        summary: MessageSummary {
            uid: entry.uid,
            seq_num: entry.uid,
            flags: vec!["\\Seen".into()],
            size: entry.size.min(u32::MAX as u64) as u32,
            date,
            envelope,
        },
        body,
    })
}

/// OPTIONS preflight for WebIMAP / WebSMTP REST routes.
pub async fn options_preflight(State(st): State<WwwState>, headers: HeaderMap) -> Response {
    let cors = st.cors_snap(&headers).await;
    cors_options_preflight(&cors)
}

/// GET `/webimap/mailboxes`
pub async fn mailboxes(State(st): State<WwwState>, headers: HeaderMap) -> Response {
    let cors = st.cors_snap(&headers).await;
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled(&cors);
    }
    let user = match webimap_authenticate(&st.app, &st.pool, &headers, &cors).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    let list = match list_user_mailboxes(&st, &user).await {
        Ok(l) => l,
        Err(e) => return json_err(StatusCode::INTERNAL_SERVER_ERROR, &e, &cors),
    };
    json_ok(StatusCode::OK, &list, &cors)
}

/// GET `/webimap/messages`
pub async fn messages(
    State(st): State<WwwState>,
    headers: HeaderMap,
    Query(q): Query<MessagesQuery>,
) -> Response {
    let cors = st.cors_snap(&headers).await;
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled(&cors);
    }
    let user = match webimap_authenticate(&st.app, &st.pool, &headers, &cors).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mailbox = q.mailbox.as_deref().unwrap_or("INBOX");
    if !user_mailbox_exists(&st, &user, mailbox).await {
        return json_err(StatusCode::BAD_REQUEST, "unknown mailbox", &cors);
    }
    let since = q.since_uid.unwrap_or(0);
    let wait = q.wait.unwrap_or(0).min(120);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(wait as u64);

    loop {
        let entries = match load_mailbox_entries(&st, &user, mailbox).await {
            Ok(e) => e,
            Err(e) => return json_err(StatusCode::BAD_REQUEST, &e, &cors),
        };
        let mut out = Vec::new();
        for entry in entries.iter().filter(|e| e.uid > since) {
            let raw = match read_blob(&st.app.mailbox_store, &user, mailbox, &entry.msg_id).await {
                Ok(b) => b,
                Err(e) => {
                    return json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string(), &cors);
                }
            };
            out.push(entry_to_summary(entry, &raw));
        }
        if !out.is_empty() || tokio::time::Instant::now() >= deadline {
            return json_ok(StatusCode::OK, &out, &cors);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// GET `/webimap/message/:uid`
pub async fn message_get(
    State(st): State<WwwState>,
    headers: HeaderMap,
    axum::extract::Path(path): axum::extract::Path<MessagePath>,
    Query(q): Query<MessageQuery>,
) -> Response {
    let cors = st.cors_snap(&headers).await;
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled(&cors);
    }
    let user = match webimap_authenticate(&st.app, &st.pool, &headers, &cors).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mailbox = q.mailbox.as_deref().unwrap_or("INBOX");
    if !user_mailbox_exists(&st, &user, mailbox).await {
        return json_err(StatusCode::BAD_REQUEST, "unknown mailbox", &cors);
    }
    let entries = match load_mailbox_entries(&st, &user, mailbox).await {
        Ok(e) => e,
        Err(e) => return json_err(StatusCode::BAD_REQUEST, &e, &cors),
    };
    let Some(entry) = find_entry(&entries, path.uid).await else {
        return json_err(StatusCode::NOT_FOUND, "message not found", &cors);
    };
    match build_detail(&st, &user, mailbox, &entry, &cors).await {
        Ok(d) => json_ok(StatusCode::OK, &d, &cors),
        Err(r) => r,
    }
}

/// DELETE `/webimap/message/:uid`
pub async fn message_delete(
    State(st): State<WwwState>,
    headers: HeaderMap,
    axum::extract::Path(path): axum::extract::Path<MessagePath>,
    Query(q): Query<MessageQuery>,
) -> Response {
    delete_by_uid(&st, headers, path.uid, q.mailbox).await
}

/// DELETE `/webimap/messages/:mailbox/:uid` (path used by app.js)
pub async fn messages_delete(
    State(st): State<WwwState>,
    headers: HeaderMap,
    axum::extract::Path(path): axum::extract::Path<MessagesDeletePath>,
) -> Response {
    delete_by_uid(&st, headers, path.uid, Some(path.mailbox)).await
}

async fn delete_by_uid(
    st: &WwwState,
    headers: HeaderMap,
    uid: u32,
    mailbox: Option<String>,
) -> Response {
    let cors = st.cors_snap(&headers).await;
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled(&cors);
    }
    let user = match webimap_authenticate(&st.app, &st.pool, &headers, &cors).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mailbox = mailbox.as_deref().unwrap_or("INBOX");
    if !user_mailbox_exists(st, &user, mailbox).await {
        return json_err(StatusCode::BAD_REQUEST, "unknown mailbox", &cors);
    }
    if let Err(e) = delete_uid(st, &user, mailbox, uid).await {
        let code = if e == "message not found" {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        return json_err(code, &e, &cors);
    }
    json_ok(StatusCode::OK, &json!({ "status": "deleted" }), &cors)
}

#[derive(Deserialize)]
pub struct FlagRequest {
    pub mailbox: String,
    pub uid: u32,
    pub flags: Vec<String>,
    pub op: String,
}

/// POST `/webimap/message/flags` — flag updates (INBOX-only maildir: acknowledged, no persistent flags).
pub async fn message_flags(
    State(st): State<WwwState>,
    headers: HeaderMap,
    Json(req): Json<FlagRequest>,
) -> Response {
    let cors = st.cors_snap(&headers).await;
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled(&cors);
    }
    let user = match webimap_authenticate(&st.app, &st.pool, &headers, &cors).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    if !user_mailbox_exists(&st, &user, &req.mailbox).await {
        return json_err(StatusCode::BAD_REQUEST, "unknown mailbox", &cors);
    }
    match req.op.as_str() {
        "add" | "remove" | "set" => json_ok(StatusCode::OK, &json!({ "status": "ok" }), &cors),
        _ => json_err(
            StatusCode::BAD_REQUEST,
            "invalid op: must be add, remove, or set",
            &cors,
        ),
    }
}

/// GET `/webimap/ws` — Madmail bidirectional WebSocket + `new_message` push.
pub async fn websocket(
    State(st): State<WwwState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    Query(q): Query<WsQuery>,
) -> impl IntoResponse {
    let cors = st.cors_snap(&headers).await;
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled(&cors);
    }
    if let Some(origin) = cors.request_origin.as_deref() {
        if cors.allows_cross_origin() && !cors.is_origin_allowed(origin) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    let st = st.clone();
    ws.on_upgrade(move |socket| async move {
        if let Err(msg) = crate::webimap_ws::run(socket, st, q).await {
            tracing::debug!(error = %msg, "webimap websocket closed");
        }
    })
}

/// List message summaries with `uid > since_uid` (WebSocket `list_messages` / push).
pub(crate) async fn summaries_since(
    st: &WwwState,
    user: &str,
    mailbox: &str,
    since_uid: u32,
) -> Result<Vec<MessageSummary>, String> {
    let entries = load_mailbox_entries(st, user, mailbox).await?;
    let mut out = Vec::new();
    for entry in entries.iter().filter(|e| e.uid > since_uid) {
        let raw = read_blob(&st.app.mailbox_store, user, mailbox, &entry.msg_id)
            .await
            .map_err(|e| e.to_string())?;
        out.push(entry_to_summary(entry, &raw));
    }
    Ok(out)
}

/// Delete a message by UID (WebSocket `delete`).
pub(crate) async fn delete_uid(
    st: &WwwState,
    user: &str,
    mailbox: &str,
    uid: u32,
) -> Result<(), String> {
    let entries = load_mailbox_entries(st, user, mailbox).await?;
    let Some(entry) = entries.iter().find(|e| e.uid == uid) else {
        return Err("message not found".into());
    };
    if mailbox.eq_ignore_ascii_case("INBOX") {
        delete_blob(&st.app.mailbox_store, user, &entry.msg_id)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        store_add_flags(
            &st.app.mailbox_store,
            user,
            mailbox,
            &entry.msg_id,
            false,
            true,
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) async fn search_messages(
    st: &WwwState,
    user: &str,
    query: &str,
) -> Result<Vec<MessageSummary>, String> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Err("empty query".into());
    }
    let entries = load_mailbox_entries(st, user, "INBOX").await?;
    let mut out = Vec::new();
    for entry in entries {
        let raw = read_blob(&st.app.mailbox_store, user, "INBOX", &entry.msg_id)
            .await
            .map_err(|e| e.to_string())?;
        let (envelope, body) = parse_envelope(&raw);
        let hay = format!(
            "{} {} {} {}",
            envelope.subject,
            envelope
                .from
                .iter()
                .map(|a| format!("{}@{}", a.mailbox, a.host))
                .collect::<Vec<_>>()
                .join(" "),
            body,
            envelope.message_id
        )
        .to_lowercase();
        if hay.contains(&q) {
            out.push(entry_to_summary(&entry, &raw));
        }
    }
    Ok(out)
}

pub(crate) async fn create_user_mailbox(
    st: &WwwState,
    user: &str,
    name: &str,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("missing mailbox name".into());
    }
    if name.eq_ignore_ascii_case("INBOX") {
        return Err("cannot create INBOX".into());
    }
    st.app
        .mailbox_store
        .init_mailbox_dir(user, name)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) async fn delete_user_mailbox(
    st: &WwwState,
    user: &str,
    name: &str,
) -> Result<(), String> {
    if name.eq_ignore_ascii_case("INBOX") {
        return Err("cannot delete INBOX".into());
    }
    if !mailbox_exists(&st.app.mailbox_store, user, name).await {
        return Err("unknown mailbox".into());
    }
    let folder = mailbox_folder(st, user, name);
    if folder.exists() {
        tokio::fs::remove_dir_all(&folder)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) async fn rename_user_mailbox(
    st: &WwwState,
    user: &str,
    old_name: &str,
    new_name: &str,
) -> Result<(), String> {
    if old_name.eq_ignore_ascii_case("INBOX") || new_name.eq_ignore_ascii_case("INBOX") {
        return Err("cannot rename INBOX".into());
    }
    if !mailbox_exists(&st.app.mailbox_store, user, old_name).await {
        return Err("unknown mailbox".into());
    }
    if mailbox_exists(&st.app.mailbox_store, user, new_name).await {
        return Err("mailbox already exists".into());
    }
    let old_folder = mailbox_folder(st, user, old_name);
    let new_folder = mailbox_folder(st, user, new_name);
    if let Some(parent) = new_folder.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::rename(&old_folder, &new_folder)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) async fn copy_uid(
    st: &WwwState,
    user: &str,
    mailbox: &str,
    uid: u32,
    dest_mailbox: &str,
) -> Result<(), String> {
    let entries = load_mailbox_entries(st, user, mailbox).await?;
    let Some(entry) = entries.iter().find(|e| e.uid == uid) else {
        return Err("message not found".into());
    };
    copy_message(
        &st.app.mailbox_store,
        user,
        mailbox,
        dest_mailbox,
        &entry.msg_id,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) async fn move_uid(
    st: &WwwState,
    user: &str,
    mailbox: &str,
    uid: u32,
    dest_mailbox: &str,
) -> Result<(), String> {
    let entries = load_mailbox_entries(st, user, mailbox).await?;
    let Some(entry) = entries.iter().find(|e| e.uid == uid) else {
        return Err("message not found".into());
    };
    move_message(
        &st.app.mailbox_store,
        user,
        mailbox,
        dest_mailbox,
        &entry.msg_id,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn mailbox_folder(st: &WwwState, user: &str, mailbox: &str) -> std::path::PathBuf {
    st.app
        .mailbox_store
        .maildir_for_mailbox(user, mailbox)
        .root
        .parent()
        .expect("mailbox folder")
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use chatmail_auth::hash_password;
    use chatmail_config::AppConfig;
    use chatmail_db::{init_memory_db, passwords, set_setting, settings_keys};
    use chatmail_state::AppState;
    use chatmail_storage::{write_blob, InboxEntry};
    use tower::ServiceExt;

    use super::*;
    use crate::www_router;

    const USER: &str = "u@x.org";
    const PASS: &str = "secret";

    fn sample_message() -> Vec<u8> {
        b"From: Alice <alice@example.org>\r\n\
          To: Bob <bob@example.org>\r\n\
          Subject: WebIMAP test\r\n\
          Date: Thu, 1 Feb 2024 10:00:00 +0000\r\n\
          Message-ID: <webimap-test@example.org>\r\n\
          Content-Type: text/plain; charset=utf-8\r\n\
          \r\n\
          Hello from WebIMAP tests\r\n"
            .to_vec()
    }

    async fn test_www_state(webimap_enabled: bool) -> (WwwState, tempfile::TempDir) {
        let pool = init_memory_db().await.unwrap();
        if webimap_enabled {
            set_setting(&pool, settings_keys::WEBIMAP_ENABLED, "true")
                .await
                .unwrap();
            set_setting(&pool, settings_keys::WEBMAIL_CORS_ORIGINS, "*")
                .await
                .unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let cfg = AppConfig::default();
        let app = Arc::new(AppState::with_quota_and_message_limit(
            dir.path(),
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &cfg,
            pool.clone(),
        ));
        let hash = hash_password(PASS).unwrap();
        passwords::create_user(&pool, USER, &hash).await.unwrap();
        app.auth.hydrate(&pool).await.unwrap();
        let st = WwwState::new(pool, app, cfg, dir.path());
        (st, dir)
    }

    async fn seed_inbox(st: &WwwState) {
        write_blob(&st.app.mailbox_store, USER, "msg-1", &sample_message())
            .await
            .unwrap();
    }

    #[test]
    fn parse_envelope_extracts_headers_and_body() {
        let raw = sample_message();
        let (env, body) = parse_envelope(&raw);
        assert_eq!(env.subject, "WebIMAP test");
        assert!(env.message_id.contains("webimap-test@example.org"));
        assert!(!env.date.is_empty());
        assert_eq!(env.from.len(), 1);
        assert_eq!(env.from[0].mailbox, "alice");
        assert_eq!(env.from[0].host, "example.org");
        assert_eq!(env.to.len(), 1);
        assert!(body.contains("Hello from WebIMAP tests"));
    }

    #[test]
    fn entry_to_summary_uses_envelope_metadata() {
        let raw = sample_message();
        let entry = InboxEntry {
            uid: 42,
            msg_id: "msg-1".into(),
            size: raw.len() as u64,
        };
        let summary = entry_to_summary(&entry, &raw);
        assert_eq!(summary.uid, 42);
        assert_eq!(summary.seq_num, 42);
        assert_eq!(summary.flags, vec!["\\Seen"]);
        assert_eq!(summary.envelope.subject, "WebIMAP test");
    }

    #[tokio::test]
    async fn find_entry_locates_uid() {
        let entries = vec![
            InboxEntry {
                uid: 1,
                msg_id: "a".into(),
                size: 1,
            },
            InboxEntry {
                uid: 2,
                msg_id: "b".into(),
                size: 2,
            },
        ];
        let found = find_entry(&entries, 2).await.unwrap();
        assert_eq!(found.msg_id, "b");
        assert!(find_entry(&entries, 99).await.is_none());
    }

    #[tokio::test]
    async fn summaries_since_and_delete_uid_roundtrip() {
        let (st, _dir) = test_www_state(true).await;
        seed_inbox(&st).await;

        let summaries = summaries_since(&st, USER, "INBOX", 0).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].envelope.subject, "WebIMAP test");

        let uid = summaries[0].uid;
        delete_uid(&st, USER, "INBOX", uid).await.unwrap();
        assert!(summaries_since(&st, USER, "INBOX", 0)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn mailboxes_disabled_returns_not_found() {
        let (st, _dir) = test_www_state(false).await;
        let app = www_router(st);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/webimap/mailboxes")
                    .header("x-email", USER)
                    .header("x-password", PASS)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn mailboxes_lists_inbox() {
        let (st, _dir) = test_www_state(true).await;
        seed_inbox(&st).await;
        let app = www_router(st);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/webimap/mailboxes")
                    .header("x-email", USER)
                    .header("x-password", PASS)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
        let inbox = body.as_array().unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0]["name"], "INBOX");
        assert_eq!(inbox[0]["messages"], 1);
    }

    #[tokio::test]
    async fn messages_and_message_get_return_mail() {
        let (st, _dir) = test_www_state(true).await;
        seed_inbox(&st).await;
        let app = www_router(st);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/webimap/messages?wait=0")
                    .header("x-email", USER)
                    .header("x-password", PASS)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let msgs: serde_json::Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
        let msgs = msgs.as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        let uid = msgs[0]["uid"].as_u64().unwrap() as u32;

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/webimap/message/{uid}"))
                    .header("x-email", USER)
                    .header("x-password", PASS)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let detail: serde_json::Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert!(detail["body"]
            .as_str()
            .unwrap()
            .contains("Hello from WebIMAP tests"));
    }

    #[tokio::test]
    async fn message_delete_removes_message() {
        let (st, _dir) = test_www_state(true).await;
        seed_inbox(&st).await;
        let app = www_router(st);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/webimap/messages?wait=0")
                    .header("x-email", USER)
                    .header("x-password", PASS)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let msgs: serde_json::Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
        let uid = msgs.as_array().unwrap()[0]["uid"].as_u64().unwrap() as u32;

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/webimap/message/{uid}"))
                    .header("x-email", USER)
                    .header("x-password", PASS)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/webimap/messages?wait=0")
                    .header("x-email", USER)
                    .header("x-password", PASS)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let msgs: serde_json::Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert!(msgs.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn message_flags_accepts_valid_ops() {
        let (st, _dir) = test_www_state(true).await;
        let app = www_router(st);
        for op in ["add", "remove", "set"] {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/webimap/message/flags")
                        .header("x-email", USER)
                        .header("x-password", PASS)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(
                            serde_json::json!({
                                "mailbox": "INBOX",
                                "uid": 1,
                                "flags": ["\\Seen"],
                                "op": op
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }
    }
}
