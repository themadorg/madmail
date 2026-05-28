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

use chatmail_config::{format_data_size, parse_data_size};
use chatmail_db::{get_setting, settings_keys};

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

#[derive(Deserialize)]
struct MessageSizeSet {
    /// Madmail-style size token (`100M`, `1G`, …). Sets both appendlimit and max_message_size.
    size: String,
}

pub async fn message_size(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => {
            let append = get_setting(&st.pool, settings_keys::APPENDLIMIT)
                .await
                .map_err(db_err)?;
            let max = get_setting(&st.pool, settings_keys::MAX_MESSAGE_SIZE)
                .await
                .map_err(db_err)?;
            let effective = st.app.message_size.effective();
            Ok((
                200,
                Some(json!({
                    "effective_bytes": effective,
                    "effective": format_data_size(effective),
                    "appendlimit": append,
                    "max_message_size": max,
                    "config_bytes": st.app.message_size.config_bytes(),
                    "config": format_data_size(st.app.message_size.config_bytes()),
                })),
            ))
        }
        "PUT" => {
            let req: MessageSizeSet = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            if req.size.trim().is_empty() {
                return Err((400, "size is required".into()));
            }
            let effective = st
                .app
                .message_size
                .set_limit(&st.pool, &st.file_config, req.size.trim())
                .await
                .map_err(|e| (400, e.to_string()))?;
            Ok((
                200,
                Some(json!({
                    "size": req.size.trim(),
                    "effective_bytes": effective,
                    "effective": format_data_size(effective),
                    "appendlimit": req.size.trim(),
                    "max_message_size": req.size.trim(),
                })),
            ))
        }
        "DELETE" => {
            let effective = st
                .app
                .message_size
                .reset_limit(&st.pool, &st.file_config)
                .await
                .map_err(db_err)?;
            Ok((
                200,
                Some(json!({
                    "reset": true,
                    "effective_bytes": effective,
                    "effective": format_data_size(effective),
                })),
            ))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}

pub fn validate_message_size_value(value: &str) -> Result<(), (u16, String)> {
    parse_data_size(value).map_err(|e| (400, e.to_string()))?;
    Ok(())
}

pub async fn refresh_message_size_after_setting(st: &AdminState, db_key: &str) {
    if db_key == settings_keys::APPENDLIMIT || db_key == settings_keys::MAX_MESSAGE_SIZE {
        let _ = st
            .app
            .message_size
            .refresh_from_db(&st.pool, &st.file_config)
            .await;
    }
}
