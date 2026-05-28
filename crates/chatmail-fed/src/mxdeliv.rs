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
use chatmail_db::federation_policy_label;
use chatmail_pgp::{enforce_encryption, EnforceOptions};
use chatmail_state::{AppState, PolicyMode};
use chatmail_storage::write_blob;
use chatmail_types::ChatmailError;
use chatmail_db::DbPool;

use chatmail_db::{inbound_local_recipient_allowed, is_federation_sender_blocked};

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
    let rcpt = header_str(headers, "x-mail-to").ok_or_else(|| {
        ChatmailError::protocol("missing X-Mail-To")
    })?;

    if !recipient_matches_server(&rcpt, &st.local_domains) {
        tracing::debug!(rcpt = %rcpt, "mxdeliv: silently dropped (not local domain)");
        return Ok(());
    }

    if is_federation_sender_blocked(&mail_from) {
        tracing::debug!(from = %mail_from, "mxdeliv: silently dropped (blocked sender)");
        return Ok(());
    }

    if !inbound_local_recipient_allowed(&st.pool, &rcpt).await? {
        tracing::debug!(rcpt = %rcpt, "mxdeliv: silently dropped (no account or reserved rcpt)");
        return Ok(());
    }

    let sender_domain = mail_from
        .rsplit_once('@')
        .map(|(_, d)| d.to_string())
        .unwrap_or_default();

    let policy_mode = PolicyMode::from_label(&federation_policy_label(&st.pool).await?);
    if !st.app.federation_policy.allows_sender(
        &sender_domain,
        &st.local_domains,
        policy_mode,
    ) {
        return Err(ChatmailError::FederationRejected(sender_domain));
    }

    enforce_encryption(
        body,
        &EnforceOptions {
            mail_from: mail_from.clone(),
            recipients: vec![rcpt.clone()],
        },
    )?;

    st.app.check_message_size(body.len())?;
    st.app.quota.check_quota(&rcpt, body.len() as u64)?;

    // Madmail: inbound HTTP counts on sender domain with empty transport (inbound_deliveries++).
    st.app.federation_tracker.record_success(&sender_domain, 0, "");

    let msg_id = uuid::Uuid::new_v4().to_string();
    write_blob(&st.app.mailbox_store, &rcpt, &msg_id, body).await?;
    st.app.quota.record_write(&rcpt, body.len() as u64);
    st.app.events.notify_new_message(&rcpt, &msg_id);

    chatmail_db::record_inbound_delivery();
    Ok(())
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
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
        let app = Arc::new(AppState::new(dir.path()));
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
        chatmail_db::db_execute!(pool, "INSERT INTO federation_rules (domain) VALUES ('evil.test')")
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path()));
        app.federation_policy.hydrate(&pool).await.unwrap();

        let st = FedState {
            pool: pool.clone(),
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
    }

    #[tokio::test]
    async fn p7_delivers_encrypted_to_local_user() {
        let pool = init_memory_db().await.unwrap();
        chatmail_db::passwords::create_user(&pool, "user@example.org", "hash")
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path()));
        app.federation_policy.hydrate(&pool).await.unwrap();

        let st = FedState {
            pool,
            app: Arc::clone(&app),
            primary_domain: "example.org".into(),
            local_domains: chatmail_types::build_local_domains("example.org", None),
        };

        let pgp = b"From: a@peer.test\r\nTo: u@example.org\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        let mut headers = HeaderMap::new();
        headers.insert("x-mail-from", "sender@peer.test".parse().unwrap());
        headers.insert("x-mail-to", "user@example.org".parse().unwrap());

        handle_mxdeliv(&st, &headers, pgp).await.unwrap();
        assert_eq!(app.quota.used_bytes("user@example.org"), pgp.len() as u64);
    }

    #[tokio::test]
    async fn p7_silently_drops_unknown_user() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::new(dir.path()));
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
        let app = Arc::new(AppState::new(dir.path()));
        app.federation_policy.hydrate(&pool).await.unwrap();

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
