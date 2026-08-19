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

//! `/admin/dkim` — outbound federation DKIM (`madmail dkim show` / `check` / `status`).

use super::AdminResult;
use crate::AdminState;

fn mail_domain(st: &AdminState) -> String {
    if st.mail_domain.is_empty() {
        st.file_config.effective_registration_domain(None)
    } else {
        st.mail_domain.clone()
    }
}

/// `GET /admin/dkim` — selector, `d=`, paths, and the TXT to publish at `default._domainkey`.
///
/// Creates the key if missing (DNS mail domain only). IP-literal domains return
/// `publishable: false` without writing a key.
pub async fn dkim(st: &AdminState, method: &str) -> AdminResult {
    match method {
        "GET" => {
            let body = chatmail_delivery::dkim::publish_info(&st.state_dir, &mail_domain(st))
                .map_err(|e| (500, e))?;
            Ok((200, Some(body)))
        }
        _ => Err((405, format!("method {method} not allowed for /admin/dkim"))),
    }
}

/// `GET /admin/dkim/check` — compare local TXT to DNS `default._domainkey`.
pub async fn dkim_check(st: &AdminState, method: &str) -> AdminResult {
    match method {
        "GET" => {
            let body = chatmail_delivery::dkim::check_dns(&st.state_dir, &mail_domain(st))
                .await
                .map_err(|e| (500, e))?;
            Ok((200, Some(body)))
        }
        _ => Err((
            405,
            format!("method {method} not allowed for /admin/dkim/check"),
        )),
    }
}

/// `GET /admin/dkim/status` — local key + DNS match; does not create a key.
pub async fn dkim_status(st: &AdminState, method: &str) -> AdminResult {
    match method {
        "GET" => {
            let body = chatmail_delivery::dkim::status_info(&st.state_dir, &mail_domain(st))
                .await
                .map_err(|e| (500, e))?;
            Ok((200, Some(body)))
        }
        _ => Err((
            405,
            format!("method {method} not allowed for /admin/dkim/status"),
        )),
    }
}
