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

//! Madmail-compatible WebIMAP WebSocket command protocol.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{broadcast::error::RecvError, Mutex};

use crate::gate::is_websmtp_enabled;
use crate::handlers::{webimap_authenticate, websmtp_deliver};
use crate::webimap::{
    build_detail, copy_uid, create_user_mailbox, delete_uid, delete_user_mailbox, find_entry,
    list_user_mailboxes, load_mailbox_entries, move_uid, rename_user_mailbox, search_messages,
    summaries_since, WsQuery,
};
use crate::WwwState;

type WsSink = futures_util::stream::SplitSink<WebSocket, Message>;

#[derive(Deserialize)]
struct WsRequest {
    req_id: Option<String>,
    action: String,
    data: Option<Value>,
}

#[derive(serde::Serialize)]
struct WsResponse {
    #[serde(skip_serializing_if = "String::is_empty")]
    req_id: String,
    action: String,
    data: Value,
}

struct WsWriter {
    sender: Arc<Mutex<WsSink>>,
}

impl WsWriter {
    async fn send_json(&self, resp: WsResponse) -> Result<(), String> {
        let text = serde_json::to_string(&resp).map_err(|e| e.to_string())?;
        self.sender
            .lock()
            .await
            .send(Message::Text(text.into()))
            .await
            .map_err(|e| e.to_string())
    }
}

pub async fn run(socket: WebSocket, st: WwwState, q: WsQuery) -> Result<(), String> {
    let user = ws_authenticate(&st.app, &st.pool, &q.email, &q.password).await?;
    let watch_mailbox = q.mailbox.unwrap_or_else(|| "INBOX".into());
    if watch_mailbox != "INBOX" {
        return Err("unknown mailbox".into());
    }
    let mut last_uid = q.since_uid.unwrap_or(0);

    let (sender, mut receiver) = socket.split();
    let writer = WsWriter {
        sender: Arc::new(Mutex::new(sender)),
    };

    let st_cmd = st.clone();
    let user_cmd = user.clone();
    let writer_cmd = WsWriter {
        sender: Arc::clone(&writer.sender),
    };
    let commands = async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    // Client keepalive (`{ "type": "ping" }`) — not a WsRequest.
                    if text.contains("\"type\":\"ping\"") || text.contains("\"type\": \"ping\"") {
                        writer_cmd
                            .send_json(WsResponse {
                                req_id: String::new(),
                                action: "pong".into(),
                                data: json!({}),
                            })
                            .await?;
                        continue;
                    }
                    let req: WsRequest = match serde_json::from_str(&text) {
                        Ok(r) => r,
                        Err(_) => {
                            writer_cmd
                                .send_json(WsResponse {
                                    req_id: String::new(),
                                    action: "error".into(),
                                    data: json!("invalid JSON"),
                                })
                                .await?;
                            continue;
                        }
                    };
                    dispatch(&st_cmd, &user_cmd, &writer_cmd, &req).await?;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        Ok::<(), String>(())
    };

    let st_push = st;
    let user_push = user;
    let writer_push = writer;
    let push = async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(2));
        let mut events = st_push.app.events.subscribe(&user_push);
        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                ev = events.recv() => {
                    match ev {
                        Ok(_) => {}
                        Err(RecvError::Lagged(_)) => {
                            st_push.app.events.record_lag();
                        }
                        // Sender may be recreated; resubscribe instead of tearing down WS.
                        Err(RecvError::Closed) => {
                            events = st_push.app.events.subscribe(&user_push);
                        }
                    }
                }
            }
            let summaries = match summaries_since(&st_push, &user_push, "INBOX", last_uid).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(user = %user_push, error = %e, "webimap push tick failed");
                    continue;
                }
            };
            for summary in summaries {
                if summary.uid > last_uid {
                    last_uid = summary.uid;
                }
                if let Err(e) = writer_push
                    .send_json(WsResponse {
                        req_id: String::new(),
                        action: "new_message".into(),
                        data: serde_json::to_value(&summary).map_err(|e| e.to_string())?,
                    })
                    .await
                {
                    tracing::debug!(user = %user_push, error = %e, "webimap push send failed");
                    return Err(e);
                }
            }
        }
    };

    tokio::select! {
        r = commands => r?,
        r = push => r?,
    }
    Ok(())
}

/// Handle one WebSocket command (testable without a live socket).
async fn handle_ws_request(st: &WwwState, user: &str, req: &WsRequest) -> WsResponse {
    let req_id = req.req_id.clone().unwrap_or_default();
    let respond = |data: Value| WsResponse {
        req_id: req_id.clone(),
        action: "result".into(),
        data,
    };
    let respond_err = |msg: &str| WsResponse {
        req_id: req_id.clone(),
        action: "error".into(),
        data: json!(msg),
    };

    let data = req.data.clone().unwrap_or(json!({}));
    match req.action.as_str() {
        "send" => {
            if !is_websmtp_enabled(&st.pool).await {
                return respond_err("send is not enabled");
            }
            #[derive(Deserialize)]
            struct SendData {
                to: Vec<String>,
                body: String,
            }
            let d: SendData = match serde_json::from_value(data) {
                Ok(d) => d,
                Err(e) => {
                    return WsResponse {
                        req_id: req_id.clone(),
                        action: "error".into(),
                        data: json!(format!("invalid send payload: {e}")),
                    };
                }
            };
            if d.to.is_empty() {
                return respond_err("missing recipients");
            }
            match websmtp_deliver(st, user, &d.to, &d.body).await {
                Ok(()) => respond(json!({ "status": "sent" })),
                Err(e) => {
                    let (_, msg) = crate::handlers::web_delivery_error(&e);
                    respond_err(&msg)
                }
            }
        }
        "fetch" => {
            #[derive(Deserialize)]
            struct FetchData {
                #[serde(default = "default_inbox")]
                mailbox: String,
                uid: u32,
            }
            let d: FetchData = match serde_json::from_value(data) {
                Ok(d) => d,
                Err(e) => {
                    return WsResponse {
                        req_id: req_id.clone(),
                        action: "error".into(),
                        data: json!(format!("invalid fetch payload: {e}")),
                    };
                }
            };
            let no_cors = crate::cors::CorsSnap::empty();
            let entries = match load_mailbox_entries(st, user, &d.mailbox).await {
                Ok(e) => e,
                Err(e) => return respond_err(&e),
            };
            let Some(entry) = find_entry(&entries, d.uid).await else {
                return respond_err("message not found");
            };
            let detail = match build_detail(st, user, &d.mailbox, &entry, &no_cors).await {
                Ok(d) => d,
                Err(_) => return respond_err("failed to load message"),
            };
            respond(serde_json::to_value(detail).unwrap_or(json!(null)))
        }
        "list_mailboxes" => match list_user_mailboxes(st, user).await {
            Ok(list) => respond(serde_json::to_value(&list).unwrap_or(json!([]))),
            Err(e) => respond_err(&e),
        },
        "list_messages" => {
            #[derive(Deserialize)]
            struct ListData {
                #[serde(default = "default_inbox")]
                mailbox: String,
                since_uid: u32,
            }
            let d: ListData = match serde_json::from_value(data) {
                Ok(d) => d,
                Err(e) => {
                    return WsResponse {
                        req_id: req_id.clone(),
                        action: "error".into(),
                        data: json!(format!("invalid list_messages payload: {e}")),
                    };
                }
            };
            match summaries_since(st, user, &d.mailbox, d.since_uid).await {
                Ok(msgs) => respond(serde_json::to_value(&msgs).unwrap_or(json!([]))),
                Err(e) => respond_err(&e),
            }
        }
        "flags" => {
            #[derive(Deserialize)]
            struct FlagsData {
                op: String,
            }
            let d: FlagsData = match serde_json::from_value(data) {
                Ok(d) => d,
                Err(e) => {
                    return WsResponse {
                        req_id: req_id.clone(),
                        action: "error".into(),
                        data: json!(format!("invalid flags payload: {e}")),
                    };
                }
            };
            match d.op.as_str() {
                "add" | "remove" | "set" => respond(json!({ "status": "ok" })),
                _ => respond_err("invalid op: must be add, remove, or set"),
            }
        }
        "delete" => {
            #[derive(Deserialize)]
            struct DeleteData {
                #[serde(default = "default_inbox")]
                mailbox: String,
                uid: u32,
            }
            let d: DeleteData = match serde_json::from_value(data) {
                Ok(d) => d,
                Err(e) => {
                    return WsResponse {
                        req_id: req_id.clone(),
                        action: "error".into(),
                        data: json!(format!("invalid delete payload: {e}")),
                    };
                }
            };
            match delete_uid(st, user, &d.mailbox, d.uid).await {
                Ok(()) => respond(json!({ "status": "deleted" })),
                Err(e) => respond_err(&e),
            }
        }
        "search" => {
            #[derive(Deserialize)]
            struct SearchData {
                query: String,
            }
            let d: SearchData = match serde_json::from_value(data) {
                Ok(d) => d,
                Err(e) => {
                    return WsResponse {
                        req_id: req_id.clone(),
                        action: "error".into(),
                        data: json!(format!("invalid search payload: {e}")),
                    };
                }
            };
            match search_messages(st, user, &d.query).await {
                Ok(msgs) => respond(serde_json::to_value(&msgs).unwrap_or(json!([]))),
                Err(e) => respond_err(&e),
            }
        }
        "create_mailbox" => {
            #[derive(Deserialize)]
            struct CreateMailboxData {
                name: String,
            }
            let d: CreateMailboxData = match serde_json::from_value(data) {
                Ok(d) => d,
                Err(e) => {
                    return WsResponse {
                        req_id: req_id.clone(),
                        action: "error".into(),
                        data: json!(format!("invalid create_mailbox payload: {e}")),
                    };
                }
            };
            match create_user_mailbox(st, user, &d.name).await {
                Ok(()) => respond(json!({ "status": "created" })),
                Err(e) => respond_err(&e),
            }
        }
        "delete_mailbox" => {
            #[derive(Deserialize)]
            struct DeleteMailboxData {
                name: String,
            }
            let d: DeleteMailboxData = match serde_json::from_value(data) {
                Ok(d) => d,
                Err(e) => {
                    return WsResponse {
                        req_id: req_id.clone(),
                        action: "error".into(),
                        data: json!(format!("invalid delete_mailbox payload: {e}")),
                    };
                }
            };
            match delete_user_mailbox(st, user, &d.name).await {
                Ok(()) => respond(json!({ "status": "deleted" })),
                Err(e) => respond_err(&e),
            }
        }
        "rename_mailbox" => {
            #[derive(Deserialize)]
            struct RenameMailboxData {
                #[serde(alias = "oldName")]
                old_name: String,
                #[serde(alias = "newName")]
                new_name: String,
            }
            let d: RenameMailboxData = match serde_json::from_value(data) {
                Ok(d) => d,
                Err(e) => {
                    return WsResponse {
                        req_id: req_id.clone(),
                        action: "error".into(),
                        data: json!(format!("invalid rename_mailbox payload: {e}")),
                    };
                }
            };
            match rename_user_mailbox(st, user, &d.old_name, &d.new_name).await {
                Ok(()) => respond(json!({ "status": "renamed" })),
                Err(e) => respond_err(&e),
            }
        }
        "copy" => {
            #[derive(Deserialize)]
            struct CopyData {
                #[serde(default = "default_inbox")]
                mailbox: String,
                uid: u32,
                #[serde(alias = "destination")]
                dest_mailbox: Option<String>,
            }
            let d: CopyData = match serde_json::from_value(data) {
                Ok(d) => d,
                Err(e) => {
                    return WsResponse {
                        req_id: req_id.clone(),
                        action: "error".into(),
                        data: json!(format!("invalid copy payload: {e}")),
                    };
                }
            };
            let Some(dest) = d.dest_mailbox.filter(|s| !s.is_empty()) else {
                return respond_err("missing dest_mailbox");
            };
            match copy_uid(st, user, &d.mailbox, d.uid, &dest).await {
                Ok(()) => respond(json!({ "status": "copied" })),
                Err(e) => respond_err(&e),
            }
        }
        "move" => {
            #[derive(Deserialize)]
            struct MoveData {
                #[serde(default = "default_inbox")]
                mailbox: String,
                uid: u32,
                #[serde(alias = "destination")]
                dest_mailbox: Option<String>,
            }
            let d: MoveData = match serde_json::from_value(data) {
                Ok(d) => d,
                Err(e) => {
                    return WsResponse {
                        req_id: req_id.clone(),
                        action: "error".into(),
                        data: json!(format!("invalid move payload: {e}")),
                    };
                }
            };
            let Some(dest) = d.dest_mailbox.filter(|s| !s.is_empty()) else {
                return respond_err("missing dest_mailbox");
            };
            match move_uid(st, user, &d.mailbox, d.uid, &dest).await {
                Ok(()) => respond(json!({ "status": "moved" })),
                Err(e) => respond_err(&e),
            }
        }
        other => respond_err(&format!("unknown action: {other}")),
    }
}

async fn dispatch(
    st: &WwwState,
    user: &str,
    writer: &WsWriter,
    req: &WsRequest,
) -> Result<(), String> {
    let resp = handle_ws_request(st, user, req).await;
    writer.send_json(resp).await?;
    Ok(())
}

fn default_inbox() -> String {
    "INBOX".into()
}

async fn ws_authenticate(
    app: &chatmail_state::AppState,
    pool: &chatmail_db::DbPool,
    email: &str,
    password: &str,
) -> Result<String, String> {
    use axum::http::{HeaderMap, HeaderValue};
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-email",
        HeaderValue::from_str(email).map_err(|e| e.to_string())?,
    );
    headers.insert(
        "x-password",
        HeaderValue::from_str(password).map_err(|e| e.to_string())?,
    );
    webimap_authenticate(app, pool, &headers, &crate::cors::CorsSnap::empty())
        .await
        .map_err(|resp| format!("auth failed ({})", resp.status()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chatmail_auth::hash_password;
    use chatmail_config::AppConfig;
    use chatmail_db::{init_memory_db, passwords, set_setting, settings_keys};
    use chatmail_state::AppState;
    use chatmail_storage::write_blob;

    use super::*;
    use crate::WwwState;

    const USER: &str = "u@x.org";
    const PASS: &str = "secret";

    fn sample_message() -> Vec<u8> {
        b"From: Alice <alice@example.org>\r\n\
          To: Bob <bob@example.org>\r\n\
          Subject: WS test\r\n\
          Date: Thu, 1 Feb 2024 10:00:00 +0000\r\n\
          Message-ID: <ws-test@example.org>\r\n\
          Content-Type: text/plain; charset=utf-8\r\n\
          \r\n\
          WebSocket body\r\n"
            .to_vec()
    }

    async fn test_www_state() -> (WwwState, tempfile::TempDir) {
        let pool = init_memory_db().await.unwrap();
        set_setting(&pool, settings_keys::WEBIMAP_ENABLED, "true")
            .await
            .unwrap();
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
        (WwwState::new(pool, app, cfg, dir.path()), dir)
    }

    async fn seed_inbox(st: &WwwState) {
        write_blob(&st.app.mailbox_store, USER, "ws-msg", &sample_message())
            .await
            .unwrap();
    }

    fn ws_req(action: &str, data: Value) -> WsRequest {
        WsRequest {
            req_id: Some("req-1".into()),
            action: action.into(),
            data: Some(data),
        }
    }

    #[test]
    fn default_inbox_used_when_mailbox_omitted() {
        #[derive(Deserialize)]
        struct ListData {
            #[serde(default = "default_inbox")]
            mailbox: String,
            #[allow(dead_code)]
            since_uid: u32,
        }
        let d: ListData = serde_json::from_value(json!({ "since_uid": 0 })).unwrap();
        assert_eq!(d.mailbox, "INBOX");
    }

    #[tokio::test]
    async fn list_mailboxes_returns_inbox() {
        let (st, _dir) = test_www_state().await;
        seed_inbox(&st).await;
        let resp = handle_ws_request(&st, USER, &ws_req("list_mailboxes", json!({}))).await;
        assert_eq!(resp.action, "result");
        assert_eq!(resp.req_id, "req-1");
        let list = resp.data.as_array().unwrap();
        assert_eq!(list[0]["name"], "INBOX");
        assert_eq!(list[0]["messages"], 1);
    }

    #[tokio::test]
    async fn list_messages_returns_summaries_since_uid() {
        let (st, _dir) = test_www_state().await;
        seed_inbox(&st).await;
        let resp = handle_ws_request(
            &st,
            USER,
            &ws_req("list_messages", json!({ "since_uid": 0 })),
        )
        .await;
        assert_eq!(resp.action, "result");
        let msgs = resp.data.as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["envelope"]["subject"], "WS test");
    }

    #[tokio::test]
    async fn fetch_returns_message_detail() {
        let (st, _dir) = test_www_state().await;
        seed_inbox(&st).await;
        let list = handle_ws_request(
            &st,
            USER,
            &ws_req("list_messages", json!({ "since_uid": 0 })),
        )
        .await;
        let uid = list.data[0]["uid"].as_u64().unwrap() as u32;

        let resp = handle_ws_request(&st, USER, &ws_req("fetch", json!({ "uid": uid }))).await;
        assert_eq!(resp.action, "result");
        assert!(resp.data["body"]
            .as_str()
            .unwrap()
            .contains("WebSocket body"));
    }

    #[tokio::test]
    async fn delete_removes_message() {
        let (st, _dir) = test_www_state().await;
        seed_inbox(&st).await;
        let uid = handle_ws_request(
            &st,
            USER,
            &ws_req("list_messages", json!({ "since_uid": 0 })),
        )
        .await
        .data[0]["uid"]
            .as_u64()
            .unwrap() as u32;

        let resp = handle_ws_request(&st, USER, &ws_req("delete", json!({ "uid": uid }))).await;
        assert_eq!(resp.action, "result");
        assert_eq!(resp.data["status"], "deleted");

        let empty = handle_ws_request(
            &st,
            USER,
            &ws_req("list_messages", json!({ "since_uid": 0 })),
        )
        .await;
        assert!(empty.data.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn send_disabled_without_websmtp() {
        let (st, _dir) = test_www_state().await;
        let resp = handle_ws_request(
            &st,
            USER,
            &ws_req("send", json!({ "to": ["u@x.org"], "body": "hi" })),
        )
        .await;
        assert_eq!(resp.action, "error");
        assert_eq!(resp.data, json!("send is not enabled"));
    }

    #[tokio::test]
    async fn unsupported_mailbox_returns_error() {
        let (st, _dir) = test_www_state().await;
        let resp = handle_ws_request(
            &st,
            USER,
            &ws_req(
                "list_messages",
                json!({ "mailbox": "Drafts", "since_uid": 0 }),
            ),
        )
        .await;
        assert_eq!(resp.action, "error");
        assert_eq!(resp.data, json!("unknown mailbox"));
    }

    #[tokio::test]
    async fn create_rename_delete_mailbox_roundtrip() {
        let (st, _dir) = test_www_state().await;
        let created = handle_ws_request(
            &st,
            USER,
            &ws_req("create_mailbox", json!({ "name": "Archive" })),
        )
        .await;
        assert_eq!(created.action, "result");
        assert_eq!(created.data["status"], "created");

        let renamed = handle_ws_request(
            &st,
            USER,
            &ws_req(
                "rename_mailbox",
                json!({ "old_name": "Archive", "new_name": "Saved" }),
            ),
        )
        .await;
        assert_eq!(renamed.action, "result");
        assert_eq!(renamed.data["status"], "renamed");

        let deleted = handle_ws_request(
            &st,
            USER,
            &ws_req("delete_mailbox", json!({ "name": "Saved" })),
        )
        .await;
        assert_eq!(deleted.action, "result");
        assert_eq!(deleted.data["status"], "deleted");
    }

    #[tokio::test]
    async fn unknown_action_returns_error() {
        let (st, _dir) = test_www_state().await;
        let resp = handle_ws_request(&st, USER, &ws_req("bogus", json!({}))).await;
        assert_eq!(resp.action, "error");
        assert!(resp
            .data
            .as_str()
            .unwrap()
            .contains("unknown action: bogus"));
    }
}
