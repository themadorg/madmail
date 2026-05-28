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
use chatmail_storage::{delete_blob, list_inbox, read_blob, InboxEntry};
use mail_parser::MessageParser;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::gate::{is_webimap_enabled, service_disabled};
use crate::handlers::webimap_authenticate;
use crate::response::{json_err, json_ok, options_preflight};
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
    env.in_reply_to = msg
        .in_reply_to()
        .as_text()
        .unwrap_or_default()
        .to_string();
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

pub(crate) async fn load_entries(st: &WwwState, user: &str) -> Result<Vec<InboxEntry>, Response> {
    list_inbox(&st.app.mailbox_store, user)
        .await
        .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))
}

pub(crate) async fn find_entry(entries: &[InboxEntry], uid: u32) -> Option<InboxEntry> {
    entries.iter().find(|e| e.uid == uid).cloned()
}

pub(crate) async fn build_detail(
    st: &WwwState,
    user: &str,
    entry: &InboxEntry,
) -> Result<MessageDetail, Response> {
    let raw = read_blob(&st.app.mailbox_store, user, "INBOX", &entry.msg_id)
        .await
        .map_err(|e| json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
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

/// OPTIONS preflight for WebIMAP REST routes.
pub async fn options() -> Response {
    options_preflight()
}

/// GET `/webimap/mailboxes`
pub async fn mailboxes(State(st): State<WwwState>, headers: HeaderMap) -> Response {
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled();
    }
    let user = match webimap_authenticate(&st.pool, &headers).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    let entries = match load_entries(&st, &user).await {
        Ok(e) => e,
        Err(r) => return r,
    };
    let unseen = entries.len() as u32;
    json_ok(
        StatusCode::OK,
        &[MailboxInfo {
            name: "INBOX".into(),
            attributes: vec![],
            messages: unseen,
            unseen,
        }],
    )
}

/// GET `/webimap/messages`
pub async fn messages(
    State(st): State<WwwState>,
    headers: HeaderMap,
    Query(q): Query<MessagesQuery>,
) -> Response {
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled();
    }
    let user = match webimap_authenticate(&st.pool, &headers).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    if q.mailbox.as_deref().is_some_and(|m| m != "INBOX") {
        return json_err(StatusCode::BAD_REQUEST, "unknown mailbox");
    }
    let since = q.since_uid.unwrap_or(0);
    let wait = q.wait.unwrap_or(0).min(120);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(wait as u64);

    loop {
        let entries = match load_entries(&st, &user).await {
            Ok(e) => e,
            Err(r) => return r,
        };
        let mut out = Vec::new();
        for entry in entries.iter().filter(|e| e.uid > since) {
            let raw = match read_blob(&st.app.mailbox_store, &user, "INBOX", &entry.msg_id).await {
                Ok(b) => b,
                Err(e) => return json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
            };
            out.push(entry_to_summary(entry, &raw));
        }
        if !out.is_empty() || tokio::time::Instant::now() >= deadline {
            return json_ok(StatusCode::OK, &out);
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
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled();
    }
    let user = match webimap_authenticate(&st.pool, &headers).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    if q.mailbox.as_deref().is_some_and(|m| m != "INBOX") {
        return json_err(StatusCode::BAD_REQUEST, "unknown mailbox");
    }
    let entries = match load_entries(&st, &user).await {
        Ok(e) => e,
        Err(r) => return r,
    };
    let Some(entry) = find_entry(&entries, path.uid).await else {
        return json_err(StatusCode::NOT_FOUND, "message not found");
    };
    match build_detail(&st, &user, &entry).await {
        Ok(d) => json_ok(StatusCode::OK, &d),
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

async fn delete_by_uid(st: &WwwState, headers: HeaderMap, uid: u32, mailbox: Option<String>) -> Response {
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled();
    }
    let user = match webimap_authenticate(&st.pool, &headers).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    if mailbox.as_deref().is_some_and(|m| m != "INBOX") {
        return json_err(StatusCode::BAD_REQUEST, "unknown mailbox");
    }
    let entries = match load_entries(&st, &user).await {
        Ok(e) => e,
        Err(r) => return r,
    };
    let Some(entry) = find_entry(&entries, uid).await else {
        return json_err(StatusCode::NOT_FOUND, "message not found");
    };
    if let Err(e) = delete_blob(&st.app.mailbox_store, &user, &entry.msg_id).await {
        return json_err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    json_ok(StatusCode::OK, &json!({ "status": "deleted" }))
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
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled();
    }
    let _user = match webimap_authenticate(&st.pool, &headers).await {
        Ok(u) => u,
        Err(r) => return r,
    };
    if req.mailbox != "INBOX" {
        return json_err(StatusCode::BAD_REQUEST, "unknown mailbox");
    }
    match req.op.as_str() {
        "add" | "remove" | "set" => json_ok(StatusCode::OK, &json!({ "status": "ok" })),
        _ => json_err(StatusCode::BAD_REQUEST, "invalid op: must be add, remove, or set"),
    }
}

/// GET `/webimap/ws` — Madmail bidirectional WebSocket + `new_message` push.
pub async fn websocket(
    State(st): State<WwwState>,
    ws: WebSocketUpgrade,
    Query(q): Query<WsQuery>,
) -> impl IntoResponse {
    if !is_webimap_enabled(&st.pool).await {
        return service_disabled();
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
    since_uid: u32,
) -> Result<Vec<MessageSummary>, String> {
    let entries = list_inbox(&st.app.mailbox_store, user)
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for entry in entries.iter().filter(|e| e.uid > since_uid) {
        let raw = read_blob(&st.app.mailbox_store, user, "INBOX", &entry.msg_id)
            .await
            .map_err(|e| e.to_string())?;
        out.push(entry_to_summary(entry, &raw));
    }
    Ok(out)
}

/// Delete a message by UID (WebSocket `delete`).
pub(crate) async fn delete_uid(st: &WwwState, user: &str, uid: u32) -> Result<(), String> {
    let entries = list_inbox(&st.app.mailbox_store, user)
        .await
        .map_err(|e| e.to_string())?;
    let Some(entry) = entries.iter().find(|e| e.uid == uid) else {
        return Err("message not found".into());
    };
    delete_blob(&st.app.mailbox_store, user, &entry.msg_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
