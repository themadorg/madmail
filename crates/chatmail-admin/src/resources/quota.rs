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

use serde::Deserialize;
use serde_json::{json, Value};

use chatmail_db::schema::quota_table;
use chatmail_db::settings_keys;

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

#[derive(Deserialize)]
struct QuotaGet {
    #[serde(default)]
    username: String,
}

#[derive(Deserialize)]
struct QuotaSet {
    #[serde(default)]
    username: String,
    max_bytes: i64,
}

pub async fn quota(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => {
            let req: QuotaGet = serde_json::from_value(body.clone()).unwrap_or(QuotaGet {
                username: String::new(),
            });
            if !req.username.is_empty() {
                let (used, max, is_default) = st.app.quota.get_quota(&req.username);
                return Ok((
                    200,
                    Some(json!({
                        "username": req.username,
                        "used_bytes": used,
                        "max_bytes": max,
                        "is_default": is_default
                    })),
                ));
            }
            let users = chatmail_db::passwords::list_users(&st.pool)
                .await
                .map_err(db_err)?;
            let total: u64 = users.iter().map(|u| st.app.quota.used_bytes(u)).sum();
            let default = st.app.quota.default_max_bytes();
            Ok((
                200,
                Some(json!({
                    "total_storage_bytes": total,
                    "accounts_count": users.len(),
                    "default_quota_bytes": default
                })),
            ))
        }
        "PUT" => {
            let req: QuotaSet = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            let user = if req.username.is_empty() {
                settings_keys::GLOBAL_QUOTA_USERNAME.to_string()
            } else {
                req.username.clone()
            };
            let now = unix_now();
            let qt = quota_table(&st.pool).await.map_err(db_err)?;
            let sql = format!(
                "INSERT INTO {qt} (username, max_storage, created_at, first_login_at, last_login_at)
                 VALUES (?, ?, ?, 0, 0)
                 ON CONFLICT(username) DO UPDATE SET max_storage = excluded.max_storage"
            );
            chatmail_db::db_execute!(
                &st.pool,
                &sql,
                user.as_str(),
                req.max_bytes,
                now
            )
            .map_err(db_err)?;
            let max = req.max_bytes.max(0) as u64;
            st.app.quota.set_max_bytes(&user, max);
            Ok((
                200,
                Some(json!({ "username": user, "max_bytes": req.max_bytes })),
            ))
        }
        "DELETE" => {
            let req: QuotaGet = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            let qt = quota_table(&st.pool).await.map_err(db_err)?;
            let sql = format!("DELETE FROM {qt} WHERE username = ?");
            chatmail_db::db_execute!(&st.pool, &sql, req.username.as_str())
            .map_err(db_err)?;
            st.app.quota.reset_max(&req.username);
            Ok((200, Some(json!({ "reset": req.username }))))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
