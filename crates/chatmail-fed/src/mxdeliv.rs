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

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use chatmail_db::{is_federation_sender_blocked, DbPool};
use chatmail_pgp::{enforce_encryption, EnforceOptions};
use chatmail_state::AppState;
use chatmail_storage::deliver_local_messages;
use chatmail_types::ChatmailError;

use crate::security::recipient_matches_server;

#[derive(Clone)]
pub struct FedState {
    pub pool: DbPool,
    pub app: Arc<AppState>,
    pub primary_domain: String,
    pub local_domains: Vec<String>,
}

/// Map handler errors to HTTP status (Madmail `chatmail.go` mxdeliv).
pub fn mxdeliv_http_status(err: &ChatmailError) -> StatusCode {
    match err {
        ChatmailError::FederationRejected(_) => StatusCode::FORBIDDEN,
        ChatmailError::EncryptionNeeded(_) => StatusCode::FORBIDDEN,
        ChatmailError::QuotaExceeded { .. } => StatusCode::INSUFFICIENT_STORAGE,
        ChatmailError::MessageTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        ChatmailError::Protocol(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn mxdeliv_handler(
    State(st): State<FedState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    match handle_mxdeliv(&st, &headers, &body).await {
        Ok(()) => (StatusCode::OK, "OK"),
        Err(e) => (mxdeliv_http_status(&e), status_body(&e)),
    }
}

fn status_body(err: &ChatmailError) -> &'static str {
    match err {
        ChatmailError::FederationRejected(_) => "Forbidden",
        ChatmailError::EncryptionNeeded(_) => "Encryption Needed: Invalid Unencrypted Mail",
        ChatmailError::QuotaExceeded { .. } => "quota",
        ChatmailError::MessageTooLarge => "message too large",
        ChatmailError::Protocol(_) => "bad request",
        _ => "error",
    }
}

async fn handle_mxdeliv(
    st: &FedState,
    headers: &HeaderMap,
    body: &[u8],
) -> chatmail_types::Result<()> {
    let mail_from = header_str(headers, "x-mail-from").unwrap_or_default();
    // One POST carries one X-Mail-To header per recipient on this server
    // (TDD 07-federation.md); deliver to each of them.
    let mut rcpts = header_strs(headers, "x-mail-to");
    if rcpts.is_empty() {
        return Err(ChatmailError::protocol("missing X-Mail-To"));
    }

    rcpts.retain(|rcpt| {
        let keep = recipient_matches_server(rcpt, &st.local_domains);
        if !keep {
            tracing::debug!(rcpt = %rcpt, "mxdeliv: silently dropped (not local domain)");
        }
        keep
    });
    if rcpts.is_empty() {
        return Ok(());
    }

    if is_federation_sender_blocked(&mail_from) {
        tracing::debug!(from = %mail_from, "mxdeliv: silently dropped (blocked sender)");
        return Ok(());
    }

    rcpts.retain(|rcpt| {
        let keep = st.app.auth.local_recipient_allowed(rcpt);
        if !keep {
            tracing::debug!(rcpt = %rcpt, "mxdeliv: silently dropped (no account or reserved rcpt)");
        }
        keep
    });
    if rcpts.is_empty() {
        return Ok(());
    }

    let sender_domain = mail_from
        .rsplit_once('@')
        .map(|(_, d)| d.to_string())
        .unwrap_or_default();

    let policy_mode = st.app.federation_policy.global_mode();
    if !st
        .app
        .federation_policy
        .allows_sender(&sender_domain, &st.local_domains, policy_mode)
    {
        return Err(ChatmailError::FederationRejected(sender_domain));
    }

    st.app.check_federation_size(body.len())?;
    st.app.check_message_size(body.len())?;

    // An over-quota recipient only fails the request when no other
    // recipient remains; erroring for all would make the remote queue
    // retry (and re-deliver) the message for recipients that are fine.
    let mut deliveries: Vec<(String, String)> = Vec::new();
    let mut quota_err = None;
    for rcpt in rcpts {
        match st.app.quota.check_quota(&rcpt, body.len() as u64) {
            Ok(()) => deliveries.push((rcpt, uuid::Uuid::new_v4().to_string())),
            Err(e) => {
                tracing::warn!(rcpt = %rcpt, error = %e, "mxdeliv: recipient over quota");
                quota_err = Some(e);
            }
        }
    }
    if deliveries.is_empty() {
        return Err(quota_err.expect("empty deliveries only after quota errors"));
    }

    enforce_encryption(
        body,
        &EnforceOptions {
            mail_from: mail_from.clone(),
            recipients: deliveries.iter().map(|(rcpt, _)| rcpt.clone()).collect(),
        },
    )?;

    // Madmail: inbound HTTP counts on sender domain with empty transport (inbound_deliveries++).
    st.app
        .federation_tracker
        .record_success(&sender_domain, 0, "");

    let outcome = deliver_local_messages(&st.app.mailbox_store, &deliveries, body).await?;
    // Notify (and charge quota) only for recipients whose body is durably
    // on disk, mirroring the SMTP session path.
    for (rcpt, msg_id) in &outcome.delivered {
        st.app.quota.record_write(rcpt, body.len() as u64);
        st.app.events.notify_new_message(rcpt, msg_id);
        st.app.notify_inbound_push(&st.pool, &mail_from, rcpt).await;
    }
    for (rcpt, _msg_id, err) in &outcome.failed {
        tracing::warn!(rcpt = %rcpt, error = %err, "mxdeliv: local delivery failed for recipient");
    }

    chatmail_db::record_inbound_delivery();
    Ok(())
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

fn header_strs(headers: &HeaderMap, name: &str) -> Vec<String> {
    headers
        .get_all(name)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use chatmail_db::init_memory_db;
    use chatmail_state::AppState;
    use std::sync::Arc;

    /// P7-UT01: federation silently drops admin-style recipients.
    #[tokio::test]
    async fn p7_ut01_test_silently_drops_admin_recipient() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.federation_policy.hydrate(&pool).await.unwrap();

        let st = FedState {
            pool,
            app,
            primary_domain: "example.org".into(),
            local_domains: chatmail_types::build_local_domains("example.org", None),
        };

        let pgp = b"From: a@peer.test\r\nTo: admin@example.org\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        let mut headers = HeaderMap::new();
        headers.insert("x-mail-from", "sender@peer.test".parse().unwrap());
        headers.insert("x-mail-to", "admin@example.org".parse().unwrap());

        handle_mxdeliv(&st, &headers, pgp).await.unwrap();
        assert_eq!(st.app.quota.used_bytes("admin@example.org"), 0);
    }

    /// P7-UT02: sender domain on blocklist is rejected under ACCEPT policy.
    #[tokio::test]
    async fn p7_ut02_test_policy_rejects_blocked_sender() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::passwords::create_user(&pool, "user@example.org", "hash")
            .await
            .unwrap();
        chatmail_db::set_federation_policy_label(&pool, "accept")
            .await
            .unwrap();
        chatmail_db::db_execute!(
            pool,
            "INSERT INTO federation_rules (domain) VALUES ('evil.test')"
        )
        .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.federation_policy.hydrate(&pool).await.unwrap();
        app.auth.hydrate(&pool).await.unwrap();

        let st = FedState {
            pool,
            app,
            primary_domain: "example.org".into(),
            local_domains: chatmail_types::build_local_domains("example.org", None),
        };

        let pgp = b"From: a@evil.test\r\nTo: user@example.org\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        let mut headers = HeaderMap::new();
        headers.insert("x-mail-from", "sender@evil.test".parse().unwrap());
        headers.insert("x-mail-to", "user@example.org".parse().unwrap());

        let err = handle_mxdeliv(&st, &headers, pgp).await.unwrap_err();
        assert!(matches!(err, ChatmailError::FederationRejected(_)));
        assert_eq!(mxdeliv_http_status(&err), StatusCode::FORBIDDEN);
    }

    /// P7-UT03: policy rejections return HTTP 403 so the remote server knows (Madmail).
    #[test]
    fn p7_ut03_test_policy_rejection_status() {
        assert_eq!(
            mxdeliv_http_status(&ChatmailError::FederationRejected("x".into())),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            mxdeliv_http_status(&ChatmailError::EncryptionNeeded("x".into())),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            mxdeliv_http_status(&ChatmailError::MessageTooLarge),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    /// P7-UT06: handler enforces `max_federation_size` before local delivery.
    #[tokio::test]
    async fn p7_ut06_rejects_body_over_federation_size() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::passwords::create_user(&pool, "user@example.org", "hash")
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let config = chatmail_config::AppConfig::default();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.federation_policy.hydrate(&pool).await.unwrap();
        app.auth.hydrate(&pool).await.unwrap();
        app.federation_size
            .set_limit(&pool, &config, "4M")
            .await
            .unwrap();

        let st = FedState {
            pool,
            app,
            primary_domain: "example.org".into(),
            local_domains: chatmail_types::build_local_domains("example.org", None),
        };

        let pgp = b"From: a@peer.test\r\nTo: user@example.org\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        let mut headers = HeaderMap::new();
        headers.insert("x-mail-from", "sender@peer.test".parse().unwrap());
        headers.insert("x-mail-to", "user@example.org".parse().unwrap());
        let oversized = vec![b'x'; 5 * 1024 * 1024];

        let err = handle_mxdeliv(&st, &headers, &oversized).await.unwrap_err();
        assert!(matches!(err, ChatmailError::MessageTooLarge));
        assert_eq!(mxdeliv_http_status(&err), StatusCode::PAYLOAD_TOO_LARGE);

        handle_mxdeliv(&st, &headers, pgp).await.unwrap();
    }

    #[tokio::test]
    async fn p7_delivers_encrypted_to_local_user() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::passwords::create_user(&pool, "user@example.org", "hash")
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.federation_policy.hydrate(&pool).await.unwrap();
        app.auth.hydrate(&pool).await.unwrap();

        let st = FedState {
            pool,
            app: Arc::clone(&app),
            primary_domain: "example.org".into(),
            local_domains: chatmail_types::build_local_domains("example.org", None),
        };

        let pgp = b"From: a@peer.test\r\nTo: user@example.org\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        let mut headers = HeaderMap::new();
        headers.insert("x-mail-from", "sender@peer.test".parse().unwrap());
        headers.insert("x-mail-to", "user@example.org".parse().unwrap());

        handle_mxdeliv(&st, &headers, pgp).await.unwrap();
        assert_eq!(app.quota.used_bytes("user@example.org"), pgp.len() as u64);
    }

    /// One POST with several X-Mail-To headers (TDD 07-federation.md) must
    /// deliver to every listed recipient, not only the first one.
    #[tokio::test]
    async fn p7_delivers_to_all_x_mail_to_recipients() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::passwords::create_user(&pool, "alice@example.org", "hash")
            .await
            .unwrap();
        chatmail_db::passwords::create_user(&pool, "bob@example.org", "hash")
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.federation_policy.hydrate(&pool).await.unwrap();
        app.auth.hydrate(&pool).await.unwrap();

        let st = FedState {
            pool,
            app: Arc::clone(&app),
            primary_domain: "example.org".into(),
            local_domains: chatmail_types::build_local_domains("example.org", None),
        };

        let pgp = b"From: a@peer.test\r\nTo: alice@example.org, bob@example.org\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        let mut headers = HeaderMap::new();
        headers.insert("x-mail-from", "sender@peer.test".parse().unwrap());
        headers.append("x-mail-to", "alice@example.org".parse().unwrap());
        headers.append("x-mail-to", "bob@example.org".parse().unwrap());
        // A dropped recipient must not affect delivery to the others.
        headers.append("x-mail-to", "ghost@example.org".parse().unwrap());

        handle_mxdeliv(&st, &headers, pgp).await.unwrap();
        assert_eq!(app.quota.used_bytes("alice@example.org"), pgp.len() as u64);
        assert_eq!(app.quota.used_bytes("bob@example.org"), pgp.len() as u64);
        assert_eq!(app.quota.used_bytes("ghost@example.org"), 0);
    }

    #[tokio::test]
    async fn p7_silently_drops_unknown_user() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.federation_policy.hydrate(&pool).await.unwrap();

        let st = FedState {
            pool,
            app,
            primary_domain: "example.org".into(),
            local_domains: chatmail_types::build_local_domains("example.org", None),
        };

        let pgp = b"From: a@peer.test\r\nTo: ghost@example.org\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        let mut headers = HeaderMap::new();
        headers.insert("x-mail-from", "sender@peer.test".parse().unwrap());
        headers.insert("x-mail-to", "ghost@example.org".parse().unwrap());

        handle_mxdeliv(&st, &headers, pgp).await.unwrap();
        assert_eq!(st.app.quota.used_bytes("ghost@example.org"), 0);
    }

    #[tokio::test]
    async fn p7_silently_drops_admin_sender() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::passwords::create_user(&pool, "user@example.org", "hash")
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path(), pool.clone()));
        app.federation_policy.hydrate(&pool).await.unwrap();
        app.auth.hydrate(&pool).await.unwrap();

        let st = FedState {
            pool,
            app,
            primary_domain: "example.org".into(),
            local_domains: chatmail_types::build_local_domains("example.org", None),
        };

        let pgp = b"From: admin@peer.test\r\nTo: user@example.org\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        let mut headers = HeaderMap::new();
        headers.insert("x-mail-from", "admin@peer.test".parse().unwrap());
        headers.insert("x-mail-to", "user@example.org".parse().unwrap());

        handle_mxdeliv(&st, &headers, pgp).await.unwrap();
        assert_eq!(st.app.quota.used_bytes("user@example.org"), 0);
    }
}
