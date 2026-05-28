//! End-to-end Secure Join message delivery (SMTP submit → inviter INBOX over IMAP).
//!
//! Mirrors relay-ping `securejoininit` + `smtpcheck` + `imapcheck`, and Delta Chat
//! Bob step 2 (`vc-request`) from `context/core/src/securejoin.rs`.

mod support;

use support::{
    build_vc_request_raw, create_user, imap_fetch_first_body, smtp_submit, spawn_mail_servers,
    webimap_send,
};

/// Joiner sends unencrypted `vc-request`; inviter must receive it in INBOX (not 523).
#[tokio::test]
async fn securejoin_vc_request_smtp_to_imap() {
    let dir = tempfile::tempdir().expect("tempdir");
    let servers = spawn_mail_servers(dir.path()).await;

    create_user(&servers.pool, "bob@test", "bob-secret").await;
    create_user(&servers.pool, "alice@test", "alice-secret").await;

    let invite_number = "relayping-invite-token-01";
    let raw = build_vc_request_raw("bob@test", "alice@test", invite_number);

    let smtp_log = smtp_submit(
        servers.smtp_addr,
        "bob@test",
        "alice@test",
        "bob@test",
        "bob-secret",
        &raw,
    )
    .await;

    assert!(
        smtp_log.contains("250 2.0.0 OK"),
        "SMTP DATA should succeed for valid Secure-Join MIME, got:\n{smtp_log}"
    );
    assert!(
        !smtp_log.contains("523"),
        "Secure-Join must not be rejected as unencrypted, got:\n{smtp_log}"
    );

    let body = imap_fetch_first_body(servers.imap_addr, "alice@test", "alice-secret").await;

    assert!(
        body.to_ascii_lowercase().contains("secure-join: vc-request"),
        "IMAP body should contain handshake line, got:\n{body}"
    );
    assert!(
        body.contains("Secure-Join: vc-request"),
        "IMAP body should contain Secure-Join header, got:\n{body}"
    );
    assert!(
        body.contains(&format!("Secure-Join-Invitenumber: {invite_number}")),
        "IMAP body should preserve invitenumber, got:\n{body}"
    );
    assert!(
        body.contains("From: <bob@test>"),
        "IMAP body should preserve From, got:\n{body}"
    );
}

/// Web app path: POST `/webimap/send` (not raw SMTP) must deliver vc-request to INBOX.
#[tokio::test]
async fn securejoin_webimap_send_delivers_vc_request() {
    let dir = tempfile::tempdir().expect("tempdir");
    let servers = spawn_mail_servers(dir.path()).await;

    create_user(&servers.pool, "bob@test", "bob-secret").await;
    create_user(&servers.pool, "alice@test", "alice-secret").await;

    let raw = build_vc_request_raw("bob@test", "alice@test", "webimap-invite-01");
    let (status, resp) = webimap_send(
        servers.http_addr,
        "bob@test",
        "bob-secret",
        &["alice@test"],
        &raw,
    )
    .await;

    assert_eq!(status, 200, "webimap send should succeed, got:\n{resp}");
    assert!(
        resp.contains("\"status\":\"sent\"") || resp.contains("sent"),
        "response should confirm send: {resp}"
    );

    let body = imap_fetch_first_body(servers.imap_addr, "alice@test", "alice-secret").await;
    assert!(
        body.contains("Secure-Join: vc-request"),
        "webimap-delivered mail missing handshake, got:\n{body}"
    );
    assert!(
        body.contains("webimap-invite-01"),
        "webimap-delivered mail missing invitenumber, got:\n{body}"
    );
}

/// Plaintext without valid Secure-Join MIME must be rejected at SMTP (Madmail PGP policy).
#[tokio::test]
async fn securejoin_rejects_plaintext_without_handshake() {
    let dir = tempfile::tempdir().expect("tempdir");
    let servers = spawn_mail_servers(dir.path()).await;

    create_user(&servers.pool, "bob@test", "bob-secret").await;
    create_user(&servers.pool, "alice@test", "alice-secret").await;

    let raw = "From: <bob@test>\r\n\
         To: <alice@test>\r\n\
         Subject: hi\r\n\
         Content-Type: text/plain\r\n\
         \r\n\
         hello\r\n";

    let smtp_log = smtp_submit(
        servers.smtp_addr,
        "bob@test",
        "alice@test",
        "bob@test",
        "bob-secret",
        raw,
    )
    .await;

    assert!(
        smtp_log.contains("523"),
        "plaintext must be rejected, got:\n{smtp_log}"
    );
}
