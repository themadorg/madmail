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
struct DnsEntry {
    lookup_key: String,
    target_host: String,
    #[serde(default)]
    comment: String,
}

#[derive(Deserialize)]
struct DnsDelete {
    lookup_key: String,
}

pub async fn dns(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => {
            let rows: Vec<(String, String, Option<String>)> = chatmail_db::db_fetch_all!(
                &st.pool,
                (String, String, Option<String>),
                "SELECT lookup_key, target_host, comment FROM dns_overrides ORDER BY lookup_key"
            )
            .map_err(db_err)?;
            let overrides: Vec<_> = rows
                .into_iter()
                .map(|(lookup_key, target_host, comment)| {
                    json!({
                        "lookup_key": lookup_key,
                        "target_host": target_host,
                        "comment": comment.unwrap_or_default()
                    })
                })
                .collect();
            Ok((
                200,
                Some(json!({ "overrides": overrides, "total": overrides.len() })),
            ))
        }
        "POST" => {
            let req: DnsEntry = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            chatmail_db::db_execute!(
                &st.pool,
                "INSERT INTO dns_overrides (lookup_key, target_host, comment)
                 VALUES (?, ?, ?)
                 ON CONFLICT(lookup_key) DO UPDATE SET
                   target_host = excluded.target_host,
                   comment = excluded.comment",
                req.lookup_key.as_str(),
                req.target_host.as_str(),
                req.comment.as_str()
            )
            .map_err(db_err)?;
            st.app.federation_policy.add_exception(&req.lookup_key);
            Ok((
                201,
                Some(json!({
                    "lookup_key": req.lookup_key,
                    "target_host": req.target_host
                })),
            ))
        }
        "DELETE" => {
            let req: DnsDelete = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            chatmail_db::db_execute!(
                &st.pool,
                "DELETE FROM dns_overrides WHERE lookup_key = ?",
                req.lookup_key.as_str()
            )
            .map_err(db_err)?;
            Ok((200, Some(json!({ "deleted": req.lookup_key }))))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}
