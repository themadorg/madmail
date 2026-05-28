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

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

#[derive(Deserialize)]
struct ExchangerBody {
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    poll_interval: Option<i64>,
}

pub async fn exchangers(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => {
            let rows: Vec<(String, String, i64, i64, Option<String>)> = chatmail_db::db_fetch_all!(
                &st.pool,
                (String, String, i64, i64, Option<String>),
                "SELECT name, url, enabled, poll_interval, last_poll_at FROM exchangers ORDER BY name"
            )
            .map_err(db_err)?;
            let list: Vec<_> = rows
                .into_iter()
                .map(|(name, url, enabled, poll_interval, last_poll_at)| {
                    json!({
                        "name": name,
                        "url": url,
                        "enabled": enabled != 0,
                        "poll_interval": poll_interval,
                        "last_poll_at": last_poll_at
                    })
                })
                .collect();
            Ok((200, Some(json!({ "exchangers": list, "total": list.len() }))))
        }
        "POST" => {
            let req: ExchangerBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            let interval = req.poll_interval.unwrap_or(60);
            chatmail_db::db_execute!(
                &st.pool,
                "INSERT INTO exchangers (name, url, enabled, poll_interval)
                 VALUES (?, ?, 1, ?)
                 ON CONFLICT(name) DO UPDATE SET url = excluded.url, poll_interval = excluded.poll_interval",
                req.name.as_str(),
                req.url.as_str(),
                interval
            )
            .map_err(db_err)?;
            Ok((201, Some(json!({ "name": req.name }))))
        }
        "PUT" => {
            let req: ExchangerBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            if let Some(en) = req.enabled {
                chatmail_db::db_execute!(
                    &st.pool,
                    "UPDATE exchangers SET enabled = ? WHERE name = ?",
                    if en { 1 } else { 0 },
                    req.name.as_str()
                )
                .map_err(db_err)?;
            }
            if req.poll_interval.is_some() || !req.url.is_empty() {
                chatmail_db::db_execute!(
                    &st.pool,
                    "UPDATE exchangers SET url = COALESCE(NULLIF(?, ''), url),
                     poll_interval = COALESCE(?, poll_interval) WHERE name = ?",
                    req.url.as_str(),
                    req.poll_interval,
                    req.name.as_str()
                )
                .map_err(db_err)?;
            }
            Ok((200, Some(json!({ "updated": req.name }))))
        }
        "DELETE" => {
            let req: ExchangerBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            chatmail_db::db_execute!(
                &st.pool,
                "DELETE FROM exchangers WHERE name = ?",
                req.name.as_str()
            )
            .map_err(db_err)?;
            Ok((200, Some(json!({ "deleted": req.name }))))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}
