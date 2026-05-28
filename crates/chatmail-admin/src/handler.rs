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

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::resources;
use crate::AdminState;

const MAX_BODY: usize = 1 << 20;

#[derive(Debug, Deserialize)]
pub struct AdminRequest {
    pub method: Option<String>,
    pub resource: String,
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub body: Value,
}

#[derive(Debug, Serialize)]
pub struct AdminResponse {
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
    pub error: Option<String>,
    pub version: String,
}

pub async fn admin_handler(
    State(st): State<AdminState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if body.len() > MAX_BODY {
        return Json(envelope(
            &st,
            413,
            None,
            None,
            Some("request body too large"),
        ));
    }

    let req: AdminRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => {
            return Json(envelope(
                &st,
                400,
                None,
                None,
                Some("invalid JSON request body"),
            ));
        }
    };

    let remote = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1");
    let inner = req.headers.unwrap_or_default();
    if !st.auth.authenticate(&inner, remote) {
        return Json(envelope(
            &st,
            401,
            None,
            None,
            Some("unauthorized"),
        ));
    }

    let method = req
        .method
        .unwrap_or_else(|| "GET".into())
        .to_ascii_uppercase();

    match resources::dispatch(&st, &method, &req.resource, &req.body).await {
        Ok((status, body)) => Json(envelope(&st, status, Some(&req.resource), body, None)),
        Err((status, msg)) => Json(envelope(
            &st,
            status,
            Some(&req.resource),
            None,
            Some(&msg),
        )),
    }
}

fn envelope(
    st: &AdminState,
    status: u16,
    resource: Option<&str>,
    body: Option<Value>,
    error: Option<&str>,
) -> AdminResponse {
    AdminResponse {
        status,
        resource: resource.map(str::to_string),
        body,
        error: error.map(str::to_string),
        version: st.version.clone(),
    }
}

