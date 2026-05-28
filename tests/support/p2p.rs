//! HTTP/WebIMAP helpers mirroring the `/app` Delta Chat web client flow.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::Instant;

use super::{build_vc_request_raw, imap_fetch_all_bodies, smtp_submit, webimap_send, MailServers};

/// Account created by `POST /new`.
#[derive(Debug, Clone)]
pub struct RegisteredUser {
    pub email: String,
    pub password: String,
}

/// `POST /new` — auto-register (same as web app connect step 1).
pub async fn http_register(http_addr: std::net::SocketAddr) -> RegisteredUser {
    let (status, body) = http_request(http_addr, "POST", "/new", &[], None).await;
    assert_eq!(status, 200, "POST /new failed: {body}");
    let email = extract_json_string(&body, "email").expect("email in /new response");
    let password = extract_json_string(&body, "password").expect("password in /new response");
    RegisteredUser { email, password }
}

/// `GET /webimap/mailboxes` — verify credentials (web app connect step 3).
pub async fn webimap_login(http_addr: std::net::SocketAddr, user: &RegisteredUser) {
    let (status, body) = http_request(
        http_addr,
        "GET",
        "/webimap/mailboxes",
        &[
            ("X-Email", user.email.as_str()),
            ("X-Password", user.password.as_str()),
        ],
        None,
    )
    .await;
    assert_eq!(
        status, 200,
        "GET /webimap/mailboxes failed for {}: {body}",
        user.email
    );
    assert!(
        body.contains("INBOX"),
        "mailboxes response should list INBOX: {body}"
    );
}

/// Madmail/Delta Chat invite URI (Alice's QR).
pub fn build_invite_uri(fingerprint: &str, invite_number: &str, auth: &str, email: &str) -> String {
    format!(
        "https://i.delta.chat/#{fingerprint}&i={invite_number}&s={auth}&a={email}"
    )
}

/// Plaintext Secure-Join step (`vc-auth-required`, `vc-contact-confirm`, …).
pub fn build_securejoin_step_raw(from: &str, to: &str, step: &str) -> String {
    let boundary = format!("sj-step-{}", step.replace('-', ""));
    let domain = from.rsplit('@').next().unwrap_or("test");
    let msg_id = format!("<sj-{step}-{domain}@{domain}>");
    format!(
        "From: <{from}>\r\n\
To: <{to}>\r\n\
Date: Tue, 6 Jan 2026 08:20:47 +0000\r\n\
Message-ID: {msg_id}\r\n\
Subject: [...]\r\n\
Chat-Version: 1.0\r\n\
Secure-Join: {step}\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\
\r\n\
--{boundary}\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Secure-Join: {step}\r\n\
\r\n\
--{boundary}--\r\n"
    )
}

/// Bob step 4b: encrypted `vc-request-with-auth` (PGP/MIME envelope; stub ciphertext).
pub fn build_vc_request_with_auth_raw(
    from: &str,
    to: &str,
    auth_token: &str,
    fingerprint: &str,
) -> String {
    let domain = from.rsplit('@').next().unwrap_or("test");
    let msg_id = format!("<sj-auth-{domain}@{domain}>");
    let stub = format!(
        "-----BEGIN PGP MESSAGE-----\r\n\
Secure-Join: vc-request-with-auth\r\n\
Secure-Join-Auth: {auth_token}\r\n\
Secure-Join-Fingerprint: {fingerprint}\r\n\
-----END PGP MESSAGE-----"
    );
    format!(
        "From: <{from}>\r\n\
To: <{to}>\r\n\
Date: Tue, 6 Jan 2026 08:20:47 +0000\r\n\
Message-ID: {msg_id}\r\n\
Subject: [...]\r\n\
Chat-Version: 1.0\r\n\
Secure-Join: vc-request-with-auth\r\n\
Secure-Join-Auth: {auth_token}\r\n\
Secure-Join-Fingerprint: {fingerprint}\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/encrypted; protocol=\"application/pgp-encrypted\"; boundary=\"pgp-auth\"\r\n\
\r\n\
--pgp-auth\r\n\
Content-Type: application/pgp-encrypted\r\n\
\r\n\
Version: 1\r\n\
\r\n\
--pgp-auth\r\n\
Content-Type: application/octet-stream\r\n\
\r\n\
{stub}\r\n\
\r\n\
--pgp-auth--\r\n"
    )
}

/// Encrypted 1:1 chat message (`app.js` `sendMessage` shape).
pub fn build_encrypted_chat_raw(from: &str, to: &str, chat_text: &str) -> String {
    let domain = from.rsplit('@').next().unwrap_or("test");
    let msg_id = format!("<chat-{domain}@{domain}>");
    let stub = format!(
        "-----BEGIN PGP MESSAGE-----\r\n\
Chat-Version: 1.0\r\n\
\r\n\
{chat_text}\r\n\
-----END PGP MESSAGE-----"
    );
    format!(
        "From: <{from}>\r\n\
To: <{to}>\r\n\
Date: Tue, 6 Jan 2026 08:20:47 +0000\r\n\
Message-ID: {msg_id}\r\n\
Subject: [...]\r\n\
Chat-Version: 1.0\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/encrypted; protocol=\"application/pgp-encrypted\"; boundary=\"pgp-boundary\"\r\n\
\r\n\
--pgp-boundary\r\n\
Content-Type: application/pgp-encrypted\r\n\
\r\n\
Version: 1\r\n\
\r\n\
--pgp-boundary\r\n\
Content-Type: application/octet-stream\r\n\
\r\n\
{stub}\r\n\
\r\n\
--pgp-boundary--\r\n"
    )
}

/// Poll IMAP INBOX until any message body contains `needle`.
pub async fn imap_wait_for_substring(
    imap_addr: std::net::SocketAddr,
    user: &RegisteredUser,
    needle: &str,
    timeout: Duration,
) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        let bodies = imap_fetch_all_bodies(imap_addr, &user.email, &user.password).await;
        for body in &bodies {
            if body.contains(needle) {
                return body.clone();
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for IMAP body containing {:?} for {}; bodies:\n{}",
                needle,
                user.email,
                bodies.join("\n---\n")
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Full web-app P2P flow: register ×2 → login → Secure Join → encrypted chat.
pub async fn run_p2p_chat_flow(servers: &MailServers) {
    let invite_number = "e2e-invite-42";
    let auth_token = "e2e-auth-secret-99";
    let alice_fp = "AABBCCDDEEFF00112233445566778899";
    let bob_fp = "112233445566778899AABBCCDDEEFF00";
    let chat_text = "Hello Alice — P2P e2e from Bob";

    // 1. Register two accounts (POST /new)
    let alice = http_register(servers.http_addr).await;
    let bob = http_register(servers.http_addr).await;
    assert_ne!(alice.email, bob.email);

    // 2. Login / verify IMAP access (GET /webimap/mailboxes)
    webimap_login(servers.http_addr, &alice).await;
    webimap_login(servers.http_addr, &bob).await;

    let _alice_uri = build_invite_uri(alice_fp, invite_number, auth_token, &alice.email);

    // 3a. Bob → Alice: vc-request (scan QR / join)
    let vc_request = build_vc_request_raw(&bob.email, &alice.email, invite_number);
    let (st, resp) = webimap_send(
        servers.http_addr,
        &bob.email,
        &bob.password,
        &[alice.email.as_str()],
        &vc_request,
    )
    .await;
    assert_eq!(st, 200, "vc-request send failed: {resp}");

    imap_wait_for_substring(
        servers.imap_addr,
        &alice,
        "Secure-Join: vc-request",
        Duration::from_secs(8),
    )
    .await;

    // 3b. Alice → Bob: vc-auth-required
    let vc_auth_required =
        build_securejoin_step_raw(&alice.email, &bob.email, "vc-auth-required");
    let (st, resp) = webimap_send(
        servers.http_addr,
        &alice.email,
        &alice.password,
        &[bob.email.as_str()],
        &vc_auth_required,
    )
    .await;
    assert_eq!(st, 200, "vc-auth-required send failed: {resp}");

    imap_wait_for_substring(
        servers.imap_addr,
        &bob,
        "vc-auth-required",
        Duration::from_secs(8),
    )
    .await;

    // 3c. Bob → Alice: vc-request-with-auth (Bob proves QR auth token)
    let vc_with_auth = build_vc_request_with_auth_raw(
        &bob.email,
        &alice.email,
        auth_token,
        bob_fp,
    );
    let (st, resp) = webimap_send(
        servers.http_addr,
        &bob.email,
        &bob.password,
        &[alice.email.as_str()],
        &vc_with_auth,
    )
    .await;
    assert_eq!(st, 200, "vc-request-with-auth send failed: {resp}");

    imap_wait_for_substring(
        servers.imap_addr,
        &alice,
        "vc-request-with-auth",
        Duration::from_secs(8),
    )
    .await;

    // 3d. Alice → Bob: vc-contact-confirm
    let confirm = build_securejoin_step_raw(&alice.email, &bob.email, "vc-contact-confirm");
    let (st, resp) = webimap_send(
        servers.http_addr,
        &alice.email,
        &alice.password,
        &[bob.email.as_str()],
        &confirm,
    )
    .await;
    assert_eq!(st, 200, "vc-contact-confirm send failed: {resp}");

    imap_wait_for_substring(
        servers.imap_addr,
        &bob,
        "vc-contact-confirm",
        Duration::from_secs(8),
    )
    .await;

    // 4. Bob → Alice: encrypted chat message
    let chat_raw = build_encrypted_chat_raw(&bob.email, &alice.email, chat_text);
    let (st, resp) = webimap_send(
        servers.http_addr,
        &bob.email,
        &bob.password,
        &[alice.email.as_str()],
        &chat_raw,
    )
    .await;
    assert_eq!(st, 200, "chat send failed: {resp}");

    let alice_body = imap_wait_for_substring(
        servers.imap_addr,
        &alice,
        chat_text,
        Duration::from_secs(8),
    )
    .await;
    assert!(
        alice_body.contains("Chat-Version: 1.0"),
        "chat mail should be Delta Chat shaped, got:\n{alice_body}"
    );
}

async fn smtp_deliver_p2p(servers: &MailServers, from: &RegisteredUser, to: &str, raw: &str) {
    let log = smtp_submit(
        servers.smtp_addr,
        &from.email,
        to,
        &from.email,
        &from.password,
        raw,
    )
    .await;
    assert!(
        log.contains("250 2.0.0 OK"),
        "SMTP delivery failed for {to}:\n{log}"
    );
}

/// Same as [`run_p2p_chat_flow`] but delivers mail via SMTP AUTH (native Delta Chat transport).
pub async fn run_p2p_chat_flow_via_smtp(servers: &MailServers) {
    let invite_number = "e2e-invite-42";
    let auth_token = "e2e-auth-secret-99";
    let alice_fp = "AABBCCDDEEFF00112233445566778899";
    let bob_fp = "112233445566778899AABBCCDDEEFF00";
    let chat_text = "Hello Alice — P2P e2e from Bob (SMTP)";

    let alice = http_register(servers.http_addr).await;
    let bob = http_register(servers.http_addr).await;
    assert_ne!(alice.email, bob.email);

    let _alice_uri = build_invite_uri(alice_fp, invite_number, auth_token, &alice.email);

    smtp_deliver_p2p(
        servers,
        &bob,
        &alice.email,
        &build_vc_request_raw(&bob.email, &alice.email, invite_number),
    )
    .await;
    imap_wait_for_substring(
        servers.imap_addr,
        &alice,
        "Secure-Join: vc-request",
        Duration::from_secs(8),
    )
    .await;

    smtp_deliver_p2p(
        servers,
        &alice,
        &bob.email,
        &build_securejoin_step_raw(&alice.email, &bob.email, "vc-auth-required"),
    )
    .await;
    imap_wait_for_substring(
        servers.imap_addr,
        &bob,
        "vc-auth-required",
        Duration::from_secs(8),
    )
    .await;

    smtp_deliver_p2p(
        servers,
        &bob,
        &alice.email,
        &build_vc_request_with_auth_raw(&bob.email, &alice.email, auth_token, bob_fp),
    )
    .await;
    imap_wait_for_substring(
        servers.imap_addr,
        &alice,
        "vc-request-with-auth",
        Duration::from_secs(8),
    )
    .await;

    smtp_deliver_p2p(
        servers,
        &alice,
        &bob.email,
        &build_securejoin_step_raw(&alice.email, &bob.email, "vc-contact-confirm"),
    )
    .await;
    imap_wait_for_substring(
        servers.imap_addr,
        &bob,
        "vc-contact-confirm",
        Duration::from_secs(8),
    )
    .await;

    smtp_deliver_p2p(
        servers,
        &bob,
        &alice.email,
        &build_encrypted_chat_raw(&bob.email, &alice.email, chat_text),
    )
    .await;
    let alice_body = imap_wait_for_substring(
        servers.imap_addr,
        &alice,
        chat_text,
        Duration::from_secs(8),
    )
    .await;
    assert!(alice_body.contains("Chat-Version: 1.0"));
}

async fn http_request(
    http_addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> (u16, String) {
    let host = http_addr.ip();
    let port = http_addr.port();
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if let Some(b) = body {
        req.push_str(&format!("Content-Length: {}\r\n", b.len()));
        req.push_str("Content-Type: application/json\r\n");
        req.push_str("\r\n");
        req.push_str(b);
    } else {
        req.push_str("\r\n");
    }

    let mut stream = TcpStream::connect(http_addr).await.expect("http connect");
    stream.write_all(req.as_bytes()).await.expect("http write");
    let mut buf = vec![0u8; 65536];
    let n = stream.read(&mut buf).await.unwrap_or(0);
    let resp = String::from_utf8_lossy(&buf[..n]).to_string();
    let status = resp
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body_start = resp.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    (status, resp[body_start..].to_string())
}

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\":\"");
    let start = json.find(&pattern)? + pattern.len();
    let rest = &json[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
