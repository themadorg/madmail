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
    build_detail, delete_uid, find_entry, load_entries, summaries_since, MailboxInfo, WsQuery,
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
    let user = ws_authenticate(&st.pool, &q.email, &q.password).await?;
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
        let mut events = st_push.app.events.subscribe();
        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                ev = events.recv() => {
                    match ev {
                        Ok(e) if e.username == user_push => {}
                        Err(RecvError::Lagged(_)) => continue,
                        Err(RecvError::Closed) => break,
                        _ => continue,
                    }
                }
            }
            let summaries = summaries_since(&st_push, &user_push, last_uid).await?;
            for summary in summaries {
                if summary.uid > last_uid {
                    last_uid = summary.uid;
                }
                writer_push
                    .send_json(WsResponse {
                        req_id: String::new(),
                        action: "new_message".into(),
                        data: serde_json::to_value(&summary).map_err(|e| e.to_string())?,
                    })
                    .await?;
            }
        }
        Ok::<(), String>(())
    };

    tokio::select! {
        r = commands => r?,
        r = push => r?,
    }
    Ok(())
}

async fn dispatch(st: &WwwState, user: &str, writer: &WsWriter, req: &WsRequest) -> Result<(), String> {
    let req_id = req.req_id.clone().unwrap_or_default();
    let respond = |data: Value| {
        writer.send_json(WsResponse {
            req_id: req_id.clone(),
            action: "result".into(),
            data,
        })
    };
    let respond_err = |msg: &str| {
        writer.send_json(WsResponse {
            req_id: req_id.clone(),
            action: "error".into(),
            data: json!(msg),
        })
    };

    let data = req.data.clone().unwrap_or(json!({}));
    match req.action.as_str() {
        "send" => {
            if !is_websmtp_enabled(&st.pool).await {
                respond_err("send is not enabled").await?;
                return Ok(());
            }
            #[derive(Deserialize)]
            struct SendData {
                to: Vec<String>,
                body: String,
            }
            let d: SendData = serde_json::from_value(data)
                .map_err(|e| format!("invalid send payload: {e}"))?;
            if d.to.is_empty() {
                respond_err("missing recipients").await?;
                return Ok(());
            }
            match websmtp_deliver(st, user, &d.to, &d.body).await {
                Ok(()) => respond(json!({ "status": "sent" })).await?,
                Err(e) => {
                    let (_, msg) = crate::handlers::web_delivery_error(&e);
                    respond_err(&msg).await?;
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
            let d: FetchData = serde_json::from_value(data)
                .map_err(|e| format!("invalid fetch payload: {e}"))?;
            if d.mailbox != "INBOX" {
                respond_err("unknown mailbox").await?;
                return Ok(());
            }
            let entries = match load_entries(st, user).await {
                Ok(e) => e,
                Err(_) => {
                    respond_err("failed to load messages").await?;
                    return Ok(());
                }
            };
            let Some(entry) = find_entry(&entries, d.uid).await else {
                respond_err("message not found").await?;
                return Ok(());
            };
            let detail = match build_detail(st, user, &entry).await {
                Ok(d) => d,
                Err(_) => {
                    respond_err("failed to load message").await?;
                    return Ok(());
                }
            };
            respond(serde_json::to_value(detail).unwrap_or(json!(null))).await?;
        }
        "list_mailboxes" => {
            let entries = match load_entries(st, user).await {
                Ok(e) => e,
                Err(_) => {
                    respond_err("failed to list mailboxes").await?;
                    return Ok(());
                }
            };
            let unseen = entries.len() as u32;
            let list = vec![MailboxInfo {
                name: "INBOX".into(),
                attributes: vec![],
                messages: unseen,
                unseen,
            }];
            respond(serde_json::to_value(&list).unwrap_or(json!([]))).await?;
        }
        "list_messages" => {
            #[derive(Deserialize)]
            struct ListData {
                #[serde(default = "default_inbox")]
                mailbox: String,
                since_uid: u32,
            }
            let d: ListData = serde_json::from_value(data)
                .map_err(|e| format!("invalid list_messages payload: {e}"))?;
            if d.mailbox != "INBOX" {
                respond_err("unknown mailbox").await?;
                return Ok(());
            }
            let msgs = summaries_since(st, user, d.since_uid).await?;
            respond(serde_json::to_value(&msgs).unwrap_or(json!([]))).await?;
        }
        "flags" => {
            #[derive(Deserialize)]
            struct FlagsData {
                op: String,
            }
            let d: FlagsData = serde_json::from_value(data)
                .map_err(|e| format!("invalid flags payload: {e}"))?;
            match d.op.as_str() {
                "add" | "remove" | "set" => respond(json!({ "status": "ok" })).await?,
                _ => respond_err("invalid op: must be add, remove, or set").await?,
            }
        }
        "delete" => {
            #[derive(Deserialize)]
            struct DeleteData {
                #[serde(default = "default_inbox")]
                mailbox: String,
                uid: u32,
            }
            let d: DeleteData = serde_json::from_value(data)
                .map_err(|e| format!("invalid delete payload: {e}"))?;
            if d.mailbox != "INBOX" {
                respond_err("unknown mailbox").await?;
                return Ok(());
            }
            match delete_uid(st, user, d.uid).await {
                Ok(()) => respond(json!({ "status": "deleted" })).await?,
                Err(e) => respond_err(&e).await?,
            }
        }
        "move" | "copy" | "search" | "create_mailbox" | "delete_mailbox" | "rename_mailbox" => {
            respond_err("not supported on this server (INBOX-only storage)").await?;
        }
        other => respond_err(&format!("unknown action: {other}")).await?,
    }
    Ok(())
}

fn default_inbox() -> String {
    "INBOX".into()
}

async fn ws_authenticate(
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
    webimap_authenticate(pool, &headers)
        .await
        .map_err(|resp| format!("auth failed ({})", resp.status()))
}
