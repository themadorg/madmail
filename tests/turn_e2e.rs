//! Phase 9 — TURN/STUN E2E (relay-ping-style IMAP + turn-rs).

mod support;

use std::time::Duration;

use chatmail_turn::{hmac_turn_password, parse_turn_metadata, turn_allocate, TurnClient};
use support::{create_user, spawn_mail_servers_opts, ImapClient, MailServersOpts};

const USER: &str = "u@test";
const PASS: &str = "imap-secret";

fn extract_turn_metadata_value(imap_response: &str) -> String {
    let marker = "/shared/vendor/deltachat/turn";
    let idx = imap_response
        .find(marker)
        .expect("turn key in METADATA response");
    let after = &imap_response[idx + marker.len()..];
    let start = after.find('"').expect("opening quote") + 1;
    let rest = &after[start..];
    let end = rest.find('"').expect("closing quote");
    rest[..end].to_string()
}

#[tokio::test]
async fn turn_imap_e2e_capability_metadata() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers_opts(dir.path(), MailServersOpts { turn: true }).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    let r = c.command("c001 CAPABILITY").await;
    assert!(r.contains("METADATA"), "CAPABILITY: {r}");
}

#[tokio::test]
async fn turn_imap_e2e_getmetadata_deltachat() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers_opts(dir.path(), MailServersOpts { turn: true }).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("m001 LOGIN {USER} {PASS}")).await;
    let r = c
        .command("m002 GETMETADATA \"\" (/shared/vendor/deltachat/turn)")
        .await;
    assert!(r.contains("METADATA"), "GETMETADATA: {r}");
    assert!(
        r.contains("/shared/vendor/deltachat/turn"),
        "expected Chatmail key: {r}"
    );

    let line = extract_turn_metadata_value(&r);
    let parsed = parse_turn_metadata(&line).expect("parse metadata line");
    assert_eq!(parsed.port, srv.turn.as_ref().unwrap().discovery.port);
    let expected_pw = hmac_turn_password(
        "integration-turn-secret",
        &parsed.expiration_timestamp.to_string(),
    )
    .unwrap();
    assert_eq!(parsed.password, expected_pw);
}

#[tokio::test]
async fn turn_imap_e2e_getmetadata_requires_auth() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers_opts(dir.path(), MailServersOpts { turn: true }).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    let r = c
        .command("m001 GETMETADATA \"\" (/shared/vendor/deltachat/turn)")
        .await;
    assert!(r.contains("NO"), "unauthenticated GETMETADATA: {r}");
    assert!(!r.contains("integration-turn-secret"));
}

#[tokio::test]
async fn turn_metadata_auth() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers_opts(dir.path(), MailServersOpts { turn: true }).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("t001 LOGIN {USER} {PASS}")).await;
    let r = c
        .command("t002 GETMETADATA \"\" (/shared/vendor/deltachat/turn)")
        .await;
    let line = extract_turn_metadata_value(&r);
    let parsed = parse_turn_metadata(&line).unwrap();

    // STUN Binding against the same turn-rs instance (smoke-level auth path).
    const BINDING_REQUEST: &[u8] = &[
        0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xA4, 0x42, 0x45, 0x58, 0x65, 0x61, 0x57, 0x53, 0x5A,
        0x6E, 0x57, 0x35, 0x76, 0x46,
    ];
    let turn_listen = srv.turn.as_ref().unwrap().listen;
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket.send_to(BINDING_REQUEST, turn_listen).await.unwrap();
    let mut buf = [0u8; 2048];
    let (n, _) = tokio::time::timeout(Duration::from_secs(2), socket.recv_from(&mut buf))
        .await
        .unwrap()
        .unwrap();
    assert!(n >= 20);
    assert_eq!(&buf[4..8], &[0x21, 0x12, 0xA4, 0x42]);

    let pw = hmac_turn_password(
        &srv.turn.as_ref().unwrap().discovery.secret,
        &parsed.expiration_timestamp.to_string(),
    )
    .unwrap();
    assert_eq!(parsed.password, pw);
}

/// IMAP credentials must succeed against the same turn-rs instance (proves relay auth path).
#[tokio::test]
async fn turn_e2e_allocate_with_imap_credentials() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers_opts(dir.path(), MailServersOpts { turn: true }).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("a001 LOGIN {USER} {PASS}")).await;
    let r = c
        .command("a002 GETMETADATA \"\" (/shared/vendor/deltachat/turn)")
        .await;
    let line = extract_turn_metadata_value(&r);
    let parsed = parse_turn_metadata(&line).unwrap();
    let turn = srv.turn.as_ref().expect("turn stack");
    let relay = turn_allocate(
        turn.listen,
        &turn.discovery.secret,
        "test",
        &parsed.expiration_timestamp.to_string(),
    )
    .await
    .expect("TURN Allocate with IMAP credentials");
    assert_ne!(
        relay.port(),
        turn.listen.port(),
        "relay port must differ from TURN control port, got {}",
        relay.port()
    );
    assert_eq!(relay.ip(), turn.listen.ip());
}

/// IMAP credentials + [RFC 8656] Send/Data relay between two allocations on embedded TURN.
#[tokio::test]
async fn turn_imap_e2e_rfc8656_relay_datapath() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers_opts(dir.path(), MailServersOpts { turn: true }).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("r001 LOGIN {USER} {PASS}")).await;
    let r = c
        .command("r002 GETMETADATA \"\" (/shared/vendor/deltachat/turn)")
        .await;
    let line = extract_turn_metadata_value(&r);
    let parsed = parse_turn_metadata(&line).unwrap();
    let turn = srv.turn.as_ref().expect("turn stack");
    let secret = &turn.discovery.secret;
    let realm = "test";
    let username = parsed.expiration_timestamp.to_string();

    let mut alice = TurnClient::new(turn.listen, secret, realm, &username)
        .await
        .expect("alice");
    let mut bob = TurnClient::new(
        turn.listen,
        secret,
        realm,
        &(parsed.expiration_timestamp + 3600).to_string(),
    )
    .await
    .expect("bob");

    let relay_a = alice.allocate().await.expect("allocate alice");
    let relay_b = bob.allocate().await.expect("allocate bob");
    assert_ne!(relay_a.port(), relay_b.port());

    alice.create_permission(relay_b).await.expect("perm a→b");
    bob.create_permission(relay_a).await.expect("perm b→a");

    alice
        .send(relay_b, b"imap-cred-relay-ok")
        .await
        .expect("send");
    let (_, data) = bob.recv_data(Duration::from_secs(3)).await.expect("recv");
    assert_eq!(data, b"imap-cred-relay-ok");
}
