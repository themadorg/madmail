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
        .check_quota(to, raw.len() as u64, "admin.notice")
        .map_err(|e| e.to_string())?;
    let msg_id = uuid::Uuid::new_v4().to_string();
    write_blob(&st.app.mailbox_store, to, &msg_id, raw)
        .await
        .map_err(|e| e.to_string())?;
    st.app.quota.record_write(to, raw.len() as u64);
    st.app.events.notify_new_message(to, &msg_id);
    st.app
        .notify_inbound_push(&st.pool, "notice@localhost", to)
        .await;
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chatmail_config::AppConfig;
    use chatmail_db::{init_memory_db, passwords, seed_install_defaults};
    use chatmail_state::AppState;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::AdminState;

    async fn test_admin_state() -> (AdminState, TempDir) {
        let pool = init_memory_db().await.unwrap();
        seed_install_defaults(&pool).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::with_quota_and_message_limit(
            dir.path(),
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &AppConfig::default(),
            pool.clone(),
        ));
        app.hydrate(&pool, &AppConfig::default()).await.unwrap();
        let st = AdminState::new(
            pool,
            app,
            AppConfig::default(),
            dir.path().to_path_buf(),
            "example.org".into(),
            "secret-token-01234567890123456789012345678901".into(),
            None,
        );
        (st, dir)
    }

    #[test]
    fn resolve_mail_domain_prefers_recipient() {
        let domain = resolve_mail_domain("alice@mail.example.org", &[]);
        assert_eq!(domain, "mail.example.org");
    }

    #[test]
    fn resolve_mail_domain_falls_back_to_first_user() {
        let users = vec!["bob@chat.example".into(), "carol@other.example".into()];
        let domain = resolve_mail_domain("", &users);
        assert_eq!(domain, "chat.example");
    }

    #[test]
    fn resolve_mail_domain_defaults_to_localhost() {
        assert_eq!(resolve_mail_domain("", &[]), "localhost");
    }

    #[test]
    fn normalize_recipient_empty_returns_empty() {
        assert_eq!(normalize_recipient("  ", "example.org"), "");
    }

    #[test]
    fn normalize_recipient_keeps_full_address() {
        assert_eq!(
            normalize_recipient("user@domain.test", "example.org"),
            "user@domain.test"
        );
    }

    #[test]
    fn normalize_recipient_appends_domain_to_localpart() {
        assert_eq!(
            normalize_recipient("alice", "example.org"),
            "alice@example.org"
        );
    }

    #[test]
    fn build_notice_message_includes_headers_and_body() {
        let raw = build_notice_message(
            "admin@example.org",
            "user@example.org",
            "Maintenance",
            "Server reboot tonight",
            "example.org",
        );
        let text = String::from_utf8(raw).unwrap();
        assert!(text.contains("From: Admin <admin@example.org>"));
        assert!(text.contains("To: user@example.org"));
        assert!(text.contains("Subject: Maintenance"));
        assert!(text.contains("Date: "));
        assert!(text.contains("Message-ID: <"));
        assert!(text.contains("Content-Type: text/plain; charset=utf-8"));
        assert!(text.contains("Server reboot tonight\n"));
    }

    #[test]
    fn build_notice_message_appends_trailing_newline_to_body() {
        let raw = build_notice_message("admin@x.org", "u@x.org", "Hi", "no newline yet", "x.org");
        let text = String::from_utf8(raw).unwrap();
        assert!(text.ends_with("no newline yet\n"));
    }

    #[tokio::test]
    async fn get_notice_reports_user_count_and_domain() {
        let (st, _dir) = test_admin_state().await;
        passwords::create_user(&st.pool, "alice@example.org", "hash")
            .await
            .unwrap();
        passwords::create_user(&st.pool, "bob@example.org", "hash")
            .await
            .unwrap();

        let (status, body) = notice(&st, "GET", &json!({})).await.unwrap();
        assert_eq!(status, 200);
        let body = body.unwrap();
        assert_eq!(body["total_users"], 2);
        assert_eq!(body["domain"], "example.org");
    }

    #[tokio::test]
    async fn post_rejects_missing_subject_or_body() {
        let (st, _dir) = test_admin_state().await;

        let err = notice(&st, "POST", &json!({ "subject": "  ", "body": "hello" }))
            .await
            .unwrap_err();
        assert_eq!(err.0, 400);
        assert!(err.1.contains("subject"));

        let err = notice(&st, "POST", &json!({ "subject": "hi", "body": "  " }))
            .await
            .unwrap_err();
        assert_eq!(err.0, 400);
        assert!(err.1.contains("body"));
    }

    #[tokio::test]
    async fn post_rejects_broadcast_when_no_users_exist() {
        let (st, _dir) = test_admin_state().await;
        let err = notice(&st, "POST", &json!({ "subject": "Hi", "body": "Everyone" }))
            .await
            .unwrap_err();
        assert_eq!(err.0, 400);
        assert!(err.1.contains("no recipients"));
    }

    #[tokio::test]
    async fn post_sends_to_explicit_recipient() {
        let (st, _dir) = test_admin_state().await;
        passwords::create_user(&st.pool, "alice@example.org", "hash")
            .await
            .unwrap();

        let (status, body) = notice(
            &st,
            "POST",
            &json!({
                "subject": "Notice",
                "body": "Hello",
                "recipient": "bob@example.org"
            }),
        )
        .await
        .unwrap();
        assert_eq!(status, 200);
        assert_eq!(body.unwrap()["sent"], 1);
    }

    #[tokio::test]
    async fn post_normalizes_localpart_recipient_with_domain() {
        let (st, _dir) = test_admin_state().await;
        passwords::create_user(&st.pool, "alice@example.org", "hash")
            .await
            .unwrap();

        let (status, body) = notice(
            &st,
            "POST",
            &json!({
                "subject": "Notice",
                "body": "Hello",
                "recipient": "bob"
            }),
        )
        .await
        .unwrap();
        assert_eq!(status, 200);
        assert_eq!(body.unwrap()["sent"], 1);
    }

    #[tokio::test]
    async fn post_broadcasts_to_all_users() {
        let (st, _dir) = test_admin_state().await;
        passwords::create_user(&st.pool, "alice@example.org", "hash")
            .await
            .unwrap();
        passwords::create_user(&st.pool, "bob@example.org", "hash")
            .await
            .unwrap();

        let (status, body) = notice(
            &st,
            "POST",
            &json!({ "subject": "Broadcast", "body": "All users" }),
        )
        .await
        .unwrap();
        assert_eq!(status, 200);
        assert_eq!(body.unwrap()["sent"], 2);
    }

    #[tokio::test]
    async fn unsupported_method_returns_405() {
        let (st, _dir) = test_admin_state().await;
        let err = notice(&st, "PATCH", &json!({})).await.unwrap_err();
        assert_eq!(err.0, 405);
        assert!(err.1.contains("PATCH"));
    }
}
