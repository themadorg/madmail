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

//! `/admin/blocklist` — Madmail `resources.BlocklistHandler`.

use serde::Deserialize;
use serde_json::{json, Value};

use chatmail_auth::normalize_username;
use chatmail_db::{blocklist, MANUAL_BLOCK_REASON};

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

#[derive(Deserialize)]
struct BlockBody {
    username: String,
    #[serde(default)]
    reason: String,
}

#[derive(Deserialize)]
struct BlockBulkBody {
    action: String,
}

fn normalize_account_username(raw: &str) -> Result<String, (u16, String)> {
    normalize_username(raw.trim()).map_err(|e| (400, e.to_string()))
}

pub async fn blocklist(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => {
            let rows = blocklist::list_blocked_users(&st.pool)
                .await
                .map_err(db_err)?;
            let blocked: Vec<_> = rows
                .into_iter()
                .map(|(username, reason, blocked_at)| {
                    json!({ "username": username, "reason": reason, "blocked_at": blocked_at })
                })
                .collect();
            let total = blocked.len();
            Ok((200, Some(json!({ "blocked": blocked, "total": total }))))
        }
        "POST" => {
            let req: BlockBody =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
            if req.username.is_empty() {
                return Err((400, "username is required".into()));
            }
            let username = normalize_account_username(&req.username)?;
            let reason = if req.reason.is_empty() {
                MANUAL_BLOCK_REASON.to_string()
            } else {
                req.reason
            };
            blocklist::block_user(&st.pool, &username, &reason)
                .await
                .map_err(db_err)?;
            st.app.auth.block(&username);
            Ok((200, Some(json!({ "blocked": username }))))
        }
        "DELETE" => {
            let req: BlockBody =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
            if req.username.is_empty() {
                return Err((400, "username is required".into()));
            }
            let username = normalize_account_username(&req.username)?;
            blocklist::unblock_user(&st.pool, &username)
                .await
                .map_err(db_err)?;
            st.app.auth.unblock(&username);
            Ok((200, Some(json!({ "unblocked": username }))))
        }
        "PATCH" => {
            let req: BlockBulkBody =
                serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
            if req.action != "delete_all" {
                return Err((
                    400,
                    format!("unknown action: {} (expected: delete_all)", req.action),
                ));
            }
            let rows = blocklist::list_blocked_users(&st.pool)
                .await
                .map_err(db_err)?;
            let mut unblocked = 0u32;
            let mut errors = Vec::new();
            for (username, _, _) in rows {
                if let Err(e) = blocklist::unblock_user(&st.pool, &username).await {
                    errors.push(format!("{username}: {e}"));
                    continue;
                }
                st.app.auth.unblock(&username);
                unblocked += 1;
            }
            let mut resp = json!({ "unblocked": unblocked });
            if !errors.is_empty() {
                resp["errors"] = json!(errors);
            }
            Ok((200, Some(resp)))
        }
        _ => Err((
            405,
            format!("method {method} not allowed for /admin/blocklist"),
        )),
    }
}
