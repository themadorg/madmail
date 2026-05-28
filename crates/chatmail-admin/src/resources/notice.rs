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

//! Admin notices — unencrypted email to one or all local users (Madmail `notice.go`).

use serde::Deserialize;
use serde_json::{json, Value};
use time::format_description::well_known::Rfc2822;
use time::OffsetDateTime;

use chatmail_db::passwords;
use chatmail_storage::write_blob;
use chatmail_types::address_domain;

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

#[derive(Deserialize)]
struct NoticeRequest {
    subject: String,
    body: String,
    #[serde(default)]
    recipient: String,
}

/// Domain for `admin@domain` and Message-ID host (from recipient or first account).
fn resolve_mail_domain(recipient: &str, users: &[String]) -> String {
    if !recipient.is_empty() {
        if let Some(d) = address_domain(recipient) {
            return d;
        }
    }
    for u in users {
        if let Some(d) = address_domain(u) {
            return d;
        }
    }
    "localhost".into()
}

fn normalize_recipient(recipient: &str, domain: &str) -> String {
    let recipient = recipient.trim();
    if recipient.is_empty() {
        return String::new();
    }
    if recipient.contains('@') {
        recipient.to_string()
    } else {
        format!("{recipient}@{domain}")
    }
}

fn build_notice_message(from: &str, to: &str, subject: &str, body: &str, domain: &str) -> Vec<u8> {
    let msg_id = uuid::Uuid::new_v4();
    let date = OffsetDateTime::now_utc()
        .format(&Rfc2822)
        .unwrap_or_else(|_| OffsetDateTime::now_utc().to_string());
    let id_host = domain.trim_matches(|c| c == '[' || c == ']');
    let mut body_text = body.to_string();
    if !body_text.ends_with('\n') {
        body_text.push('\n');
    }
    format!(
        "From: Admin <{from}>\r\n\
         To: {to}\r\n\
         Subject: {subject}\r\n\
         Date: {date}\r\n\
         Message-ID: <{msg_id}@{id_host}>\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         MIME-Version: 1.0\r\n\
         \r\n\
         {body_text}"
    )
    .into_bytes()
}

async fn deliver_notice(st: &AdminState, to: &str, raw: &[u8]) -> Result<(), String> {
    st.app
        .quota
        .check_quota(to, raw.len() as u64)
        .map_err(|e| e.to_string())?;
    let msg_id = uuid::Uuid::new_v4().to_string();
    write_blob(&st.app.mailbox_store, to, &msg_id, raw)
        .await
        .map_err(|e| e.to_string())?;
    st.app.quota.record_write(to, raw.len() as u64);
    st.app.events.notify_new_message(to, &msg_id);
    Ok(())
}

pub async fn notice(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => {
            let users = passwords::list_users(&st.pool).await.map_err(db_err)?;
            let domain = resolve_mail_domain("", &users);
            Ok((
                200,
                Some(json!({
                    "total_users": users.len(),
                    "domain": domain,
                })),
            ))
        }
        "POST" => {
            let req: NoticeRequest = serde_json::from_value(body.clone())
                .map_err(|e| (400, format!("invalid request body: {e}")))?;
            if req.subject.trim().is_empty() {
                return Err((400, "subject is required".into()));
            }
            if req.body.trim().is_empty() {
                return Err((400, "body is required".into()));
            }

            let all_users = passwords::list_users(&st.pool).await.map_err(db_err)?;
            let domain = resolve_mail_domain(&req.recipient, &all_users);
            let sender = format!("admin@{domain}");

            let recipients: Vec<String> = if req.recipient.trim().is_empty() {
                all_users
            } else {
                vec![normalize_recipient(&req.recipient, &domain)]
            };

            if recipients.is_empty() {
                return Err((400, "no recipients found".into()));
            }

            let mut sent = 0i32;
            let mut failed = 0i32;
            let mut errors: Vec<String> = Vec::new();

            for rcpt in recipients {
                let raw = build_notice_message(&sender, &rcpt, &req.subject, &req.body, &domain);
                match deliver_notice(st, &rcpt, &raw).await {
                    Ok(()) => sent += 1,
                    Err(e) => {
                        failed += 1;
                        errors.push(format!("{rcpt}: {e}"));
                    }
                }
            }

            let status = if sent == 0 && failed > 0 { 500 } else { 200 };
            let mut resp = json!({ "sent": sent, "failed": failed });
            if !errors.is_empty() {
                resp["errors"] = json!(errors);
            }
            Ok((status, Some(resp)))
        }
        _ => Err((405, format!("method {method} not allowed"))),
    }
}
