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

//! Queue admin API — maildir / blob purge (Madmail `resources/queue.go`).
//!
//! madmail-v2 has no persistent SMTP retry queue (outbound is in-memory). This endpoint
//! manages on-disk message storage under `{state_dir}/mail/`, not a delivery queue.

use std::time::Duration;

use chatmail_storage::{
    prune_unread_older, purge_all_mail_blobs, purge_mail_blobs_older, purge_read_messages,
    purge_user_messages,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::AdminResult;
use crate::AdminState;

#[derive(Deserialize)]
struct QueueRequest {
    action: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    retention: String,
}

fn parse_retention(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("retention is required (e.g. \"1h\", \"72h\")".into());
    }
    if let Some(h) = s.strip_suffix('h') {
        let n: u64 = h
            .trim()
            .parse()
            .map_err(|e| format!("invalid retention: {e}"))?;
        return Ok(Duration::from_secs(n * 3600));
    }
    if let Some(m) = s.strip_suffix('m') {
        let n: u64 = m
            .trim()
            .parse()
            .map_err(|e| format!("invalid retention: {e}"))?;
        return Ok(Duration::from_secs(n * 60));
    }
    if let Some(sec) = s.strip_suffix('s') {
        let n: u64 = sec
            .trim()
            .parse()
            .map_err(|e| format!("invalid retention: {e}"))?;
        return Ok(Duration::from_secs(n));
    }
    let n: u64 = s.parse().map_err(|e| format!("invalid retention: {e}"))?;
    Ok(Duration::from_secs(n))
}

pub async fn queue(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    if method != "POST" {
        return Err((405, format!("method {method} not allowed, use POST")));
    }

    let req: QueueRequest = serde_json::from_value(body.clone())
        .map_err(|e| (400, format!("invalid request body: {e}")))?;

    let store = &st.app.mailbox_store;
    let mail_dir = store.state_dir().join("mail");

    match req.action.as_str() {
        "purge_user" => {
            if req.username.trim().is_empty() {
                return Err((400, "username is required for purge_user".into()));
            }
            let deleted = purge_user_messages(store, &req.username)
                .await
                .map_err(|e| (500, e.to_string()))?;
            Ok((
                200,
                Some(json!({
                    "action": "purge_user",
                    "message": format!("purged messages for user {}", req.username),
                    "deleted": deleted,
                })),
            ))
        }
        "purge_all" => {
            let deleted = purge_all_mail_blobs(store)
                .await
                .map_err(|e| (500, e.to_string()))?;
            Ok((
                200,
                Some(json!({
                    "action": "purge_all",
                    "message": "purged all message files from mail storage",
                    "deleted": deleted,
                })),
            ))
        }
        "purge_read" | "purge_read_blobs" => {
            let deleted = purge_read_messages(store)
                .await
                .map_err(|e| (500, e.to_string()))?;
            Ok((
                200,
                Some(json!({
                    "action": req.action,
                    "message": "purged seen messages (maildir cur/)",
                    "deleted": deleted,
                })),
            ))
        }
        "purge_older" => {
            let retention = parse_retention(&req.retention).map_err(|e| (400, e))?;
            if retention.is_zero() {
                return Err((400, "retention must be positive".into()));
            }
            let deleted = prune_unread_older(store, retention)
                .await
                .map_err(|e| (500, e.to_string()))?;
            Ok((
                200,
                Some(json!({
                    "action": "purge_older",
                    "message": format!("pruned unread messages older than {:?}", retention),
                    "deleted": deleted,
                })),
            ))
        }
        "purge_blobs" => {
            let deleted = purge_all_mail_blobs(store)
                .await
                .map_err(|e| (500, e.to_string()))?;
            Ok((
                200,
                Some(json!({
                    "action": "purge_blobs",
                    "message": format!("deleted {deleted} entries from {}", mail_dir.display()),
                    "deleted": deleted,
                })),
            ))
        }
        "purge_blobs_older" => {
            let retention = parse_retention(&req.retention).map_err(|e| (400, e))?;
            if retention.is_zero() {
                return Err((400, "retention must be positive".into()));
            }
            let deleted = purge_mail_blobs_older(store, retention)
                .await
                .map_err(|e| (500, e.to_string()))?;
            Ok((
                200,
                Some(json!({
                    "action": "purge_blobs_older",
                    "message": format!(
                        "deleted {deleted} entries older than {:?} from {}",
                        retention,
                        mail_dir.display()
                    ),
                    "deleted": deleted,
                })),
            ))
        }
        "purge_queue" => {
            let dir = st.state_dir.join("remote_queue");
            let store = chatmail_delivery::QueueStore::new(dir);
            let deleted = store.purge_all().await.map_err(|e| (500, e.to_string()))?;
            Ok((
                200,
                Some(json!({
                    "action": "purge_queue",
                    "message": format!("removed {deleted} outbound queue entries"),
                    "deleted": deleted,
                })),
            ))
        }
        other => Err((400, format!("unknown action: {other}"))),
    }
}
