//! SMTP/IMAP test clients (relay-ping style) for integration tests.

#![allow(dead_code, unused_imports)]

pub mod imap_client;
pub mod p2p;

pub use imap_client::{pgp_mime_for_user, ImapClient, PGP_MIME_BODY};
pub use p2p::{
    build_encrypted_chat_raw, build_invite_uri, build_securejoin_step_raw,
    build_vc_request_with_auth_raw, http_register, imap_wait_for_substring, run_p2p_chat_flow,
    run_p2p_chat_flow_via_smtp, webimap_login, RegisteredUser,
};

use std::net::TcpListener as StdListener;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use base64::Engine;
use chatmail_auth::hash_password;
use chatmail_config::AppConfig;
use chatmail_db::DbPool;
use chatmail_delivery::{start_outbound_queue, DeliveryContext};
use chatmail_imap::{ImapSession, ImapSessionConfig};
use chatmail_smtp::{SmtpSession, SmtpSessionConfig};
use chatmail_state::AppState;
use chatmail_storage::write_blob;
use chatmail_turn::{spawn_turn_server_with_opts, TurnDiscovery, TurnServerHandle, TurnSpawnOpts};
use chatmail_www::{www_router, WwwState};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Unencrypted `vc-request` MIME (relay-ping `buildVCRequest` / Delta Chat Bob).
pub fn build_vc_request_raw(from: &str, to: &str, invite_number: &str) -> String {
    let boundary = format!("securejoin-{invite_number}");
    let domain = from.rsplit('@').next().unwrap_or("test");
    let msg_id = format!("<sj-{invite_number}@{domain}>");
    format!(
        "From: <{from}>\r\n\
To: <{to}>\r\n\
Date: Tue, 6 Jan 2026 08:20:47 +0000\r\n\
Message-ID: {msg_id}\r\n\
Subject: [...]\r\n\
Chat-Version: 1.0\r\n\
Secure-Join: vc-request\r\n\
Secure-Join-Invitenumber: {invite_number}\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\
\r\n\
--{boundary}\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
secure-join: vc-request\r\n\
\r\n\
--{boundary}--\r\n"
    )
}

pub struct MailServers {
    pub smtp_addr: std::net::SocketAddr,
    pub imap_addr: std::net::SocketAddr,
    pub http_addr: std::net::SocketAddr,
    pub pool: DbPool,
    pub ctx: Arc<AppState>,
    pub turn: Option<TurnTestStack>,
}

/// Running TURN + discovery settings for integration tests.
pub struct TurnTestStack {
    pub discovery: TurnDiscovery,
    pub listen: std::net::SocketAddr,
    pub _server: TurnServerHandle,
}

#[derive(Clone, Copy, Default)]
pub struct MailServersOpts {
    pub turn: bool,
}

pub async fn spawn_mail_servers(dir: &std::path::Path) -> MailServers {
    spawn_mail_servers_opts(dir, MailServersOpts::default()).await
}

pub async fn spawn_mail_servers_opts(dir: &std::path::Path, opts: MailServersOpts) -> MailServers {
    let pool = chatmail_db::init_memory_db().await.expect("db");
    chatmail_db::set_setting(&pool, chatmail_db::settings_keys::WEBIMAP_ENABLED, "true")
        .await
        .expect("webimap");
    chatmail_db::set_setting(&pool, chatmail_db::settings_keys::WEBSMTP_ENABLED, "true")
        .await
        .expect("websmtp");
    let ctx = Arc::new(AppState::new(dir));

    let app_config = AppConfig {
        hostname: Some("test".into()),
        primary_domain: Some("test".into()),
        ..Default::default()
    };
    let hostname = "test".to_string();
    let local_domains = app_config.effective_local_domains(&hostname);
    let delivery = DeliveryContext {
        pool: pool.clone(),
        state: Arc::clone(&ctx),
        primary_domain: "test".into(),
        local_domains: local_domains.clone(),
    };
    start_outbound_queue(delivery, dir, &app_config.queue)
        .await
        .expect("outbound queue");

    let smtp_listener = StdListener::bind("127.0.0.1:0").expect("smtp bind");
    smtp_listener.set_nonblocking(true).expect("smtp nb");
    let smtp_addr = smtp_listener.local_addr().expect("smtp addr");

    let imap_listener = StdListener::bind("127.0.0.1:0").expect("imap bind");
    imap_listener.set_nonblocking(true).expect("imap nb");
    let imap_addr = imap_listener.local_addr().expect("imap addr");

    let smtp_cfg = SmtpSessionConfig {
        hostname: "mx.test".into(),
        primary_domain: "test".into(),
        local_domains: vec!["test".into()],
        jit_domain: None,
        credential_policy: chatmail_config::CredentialPolicy::default(),
        require_auth: true,
        module: "submission",
    };

    let pool_smtp = pool.clone();
    let ctx_smtp = Arc::clone(&ctx);
    let cfg_smtp = smtp_cfg.clone();
    tokio::spawn(async move {
        let listener = TcpListener::from_std(smtp_listener).expect("smtp tokio");
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let pool = pool_smtp.clone();
            let ctx = Arc::clone(&ctx_smtp);
            let cfg = cfg_smtp.clone();
            tokio::spawn(async move {
                let mut session = SmtpSession::new(ctx, pool, cfg);
                let _ = session.handle_connection(stream).await;
            });
        }
    });

    let turn_stack = if opts.turn {
        let secret = "integration-turn-secret";
        let turn_listen = {
            let s = StdListener::bind("127.0.0.1:0").expect("turn bind");
            let addr = s.local_addr().expect("turn addr");
            drop(s);
            addr
        };
        let server = spawn_turn_server_with_opts(
            secret,
            "test",
            turn_listen,
            turn_listen,
            TurnSpawnOpts::for_tests(),
        )
        .await
        .expect("turn server");
        let discovery = TurnDiscovery {
            server: turn_listen.ip().to_string(),
            port: turn_listen.port(),
            secret: secret.into(),
            ttl_secs: 3600,
            turn_test_relay_only: false,
        };
        Some(TurnTestStack {
            discovery,
            listen: turn_listen,
            _server: server,
        })
    } else {
        None
    };

    let turn_imap = turn_stack.as_ref().map(|t| t.discovery.clone());

    let pool_imap = pool.clone();
    let ctx_imap = Arc::clone(&ctx);
    tokio::spawn(async move {
        let listener = TcpListener::from_std(imap_listener).expect("imap tokio");
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let pool = pool_imap.clone();
            let ctx = Arc::clone(&ctx_imap);
            let turn = turn_imap.clone();
            tokio::spawn(async move {
                let mut session = ImapSession::new(
                    ctx,
                    pool,
                    ImapSessionConfig {
                        hostname: "imap.test".into(),
                        primary_domain: "test".into(),
                        jit_domain: None,
                        credential_policy: chatmail_config::CredentialPolicy::default(),
                        turn,
                        iroh: None,
                    },
                );
                let _ = session.handle_connection(stream).await;
            });
        }
    });

    let http_listener = StdListener::bind("127.0.0.1:0").expect("http bind");
    http_listener.set_nonblocking(true).expect("http nb");
    let http_addr = http_listener.local_addr().expect("http addr");

    let www_state = WwwState::new(pool.clone(), Arc::clone(&ctx), app_config);
    let router: Router = www_router(www_state);
    tokio::spawn(async move {
        let listener = TcpListener::from_std(http_listener).expect("http tokio");
        let _ = axum::serve(listener, router).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    MailServers {
        smtp_addr,
        imap_addr,
        http_addr,
        pool,
        ctx,
        turn: turn_stack,
    }
}

/// POST `/webimap/send` (web app transport) with X-Email / X-Password auth.
pub async fn webimap_send(
    http_addr: std::net::SocketAddr,
    from: &str,
    password: &str,
    to: &[&str],
    raw: &str,
) -> (u16, String) {
    let payload = serde_json::json!({
        "from": from,
        "to": to,
        "body": raw,
    });
    let payload = serde_json::to_string(&payload).expect("json");
    let host = http_addr.ip();
    let port = http_addr.port();
    let req = format!(
        "POST /webimap/send HTTP/1.1\r\n\
Host: {host}:{port}\r\n\
X-Email: {from}\r\n\
X-Password: {password}\r\n\
Content-Type: application/json\r\n\
Content-Length: {}\r\n\
\r\n\
{payload}",
        payload.len(),
    );

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
    (status, resp)
}

/// Write a message to the user's maildir and notify IMAP IDLE waiters.
pub async fn deliver_message(ctx: &AppState, user: &str, msg_id: &str, body: &[u8]) {
    write_blob(&ctx.mailbox_store, user, msg_id, body)
        .await
        .expect("write_blob");
    ctx.events.notify_new_message(user, msg_id);
}

pub async fn create_user(ctx: &AppState, pool: &DbPool, email: &str, password: &str) {
    let hash = hash_password(password).expect("hash");
    chatmail_db::passwords::create_user(pool, email, &hash)
        .await
        .expect("create user");
    ctx.auth.insert(email, &hash);
}

/// Submit a message via SMTP AUTH PLAIN (relay-ping `sendVCRequest` flow).
pub async fn smtp_submit(
    addr: std::net::SocketAddr,
    mail_from: &str,
    rcpt: &str,
    username: &str,
    password: &str,
    raw: &str,
) -> String {
    let mut stream = TcpStream::connect(addr).await.expect("smtp connect");
    let mut buf = [0u8; 4096];
    let mut transcript = String::new();

    transcript.push_str(&read_smtp(&mut stream, &mut buf).await);
    send_smtp(&mut stream, "EHLO relay-ping.test").await;
    transcript.push_str(&read_smtp(&mut stream, &mut buf).await);

    let cred = format!("\0{username}\0{password}");
    let b64 = base64::engine::general_purpose::STANDARD.encode(cred.as_bytes());
    send_smtp(&mut stream, &format!("AUTH PLAIN {b64}")).await;
    transcript.push_str(&read_smtp(&mut stream, &mut buf).await);

    send_smtp(&mut stream, &format!("MAIL FROM:<{mail_from}>")).await;
    transcript.push_str(&read_smtp(&mut stream, &mut buf).await);
    send_smtp(&mut stream, &format!("RCPT TO:<{rcpt}>")).await;
    transcript.push_str(&read_smtp(&mut stream, &mut buf).await);
    send_smtp(&mut stream, "DATA").await;
    transcript.push_str(&read_smtp(&mut stream, &mut buf).await);

    for line in raw.split("\r\n") {
        if line == "." {
            continue;
        }
        let escaped = if line.starts_with('.') {
            format!(".{line}")
        } else {
            line.to_string()
        };
        send_smtp(&mut stream, &escaped).await;
    }
    send_smtp(&mut stream, ".").await;
    transcript.push_str(&read_smtp(&mut stream, &mut buf).await);
    send_smtp(&mut stream, "QUIT").await;
    transcript.push_str(&read_smtp(&mut stream, &mut buf).await);
    transcript
}

/// Fetch the first message body from INBOX via IMAP (LOGIN → SELECT → UID FETCH BODY[]).
pub async fn imap_fetch_first_body(
    addr: std::net::SocketAddr,
    username: &str,
    password: &str,
) -> String {
    imap_fetch_all_bodies(addr, username, password)
        .await
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Fetch all message bodies from INBOX (UID FETCH 1:* BODY[]).
pub async fn imap_fetch_all_bodies(
    addr: std::net::SocketAddr,
    username: &str,
    password: &str,
) -> Vec<String> {
    let mut c = ImapClient::connect(addr).await;
    c.command(&format!("a001 LOGIN {username} {password}"))
        .await;
    c.command("a002 SELECT INBOX").await;
    let fetch = c.command("a003 UID FETCH 1:* (UID BODY.PEEK[])").await;
    extract_all_literal_bodies(&fetch)
}

fn extract_literal_body(transcript: &str) -> String {
    extract_all_literal_bodies(transcript)
        .into_iter()
        .next()
        .unwrap_or_else(|| transcript.to_string())
}

fn extract_all_literal_bodies(transcript: &str) -> Vec<String> {
    transcript
        .match_indices("BODY[] {")
        .filter_map(|(idx, _)| extract_literal_at(transcript, idx))
        .collect()
}

fn extract_literal_at(transcript: &str, idx: usize) -> Option<String> {
    let rest = &transcript[idx..];
    let start = rest.find('{')?;
    let end = rest[start + 1..].find('}')?;
    let len: usize = rest[start + 1..start + 1 + end].parse().ok()?;
    let after = &rest[start + 1 + end + 1..];
    let nl = after.find('\n')?;
    let body = &after[nl + 1..];
    let take = len.min(body.len());
    Some(body[..take].to_string())
}

async fn send_smtp(stream: &mut TcpStream, line: &str) {
    stream.write_all(line.as_bytes()).await.expect("smtp write");
    stream.write_all(b"\r\n").await.expect("smtp crlf");
}

async fn send_imap(stream: &mut TcpStream, line: &str) {
    stream.write_all(line.as_bytes()).await.expect("imap write");
    stream.write_all(b"\r\n").await.expect("imap crlf");
}

async fn read_smtp(stream: &mut TcpStream, buf: &mut [u8]) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut acc = String::new();
        for _ in 0..32 {
            let n = stream.read(buf).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            acc.push_str(&String::from_utf8_lossy(&buf[..n]));
            if acc.contains("250 ")
                || acc.contains("235 ")
                || acc.contains("354 ")
                || acc.contains("523 ")
                || acc.contains("554 ")
                || acc.contains("221 ")
            {
                break;
            }
        }
        acc
    })
    .await
    .unwrap_or_default()
}

async fn read_until(stream: &mut TcpStream, buf: &mut [u8], needle: &str) -> String {
    let mut acc = String::new();
    for _ in 0..80 {
        let n = stream.read(buf).await.unwrap_or(0);
        if n > 0 {
            acc.push_str(&String::from_utf8_lossy(&buf[..n]));
            if acc.contains(needle) {
                return acc;
            }
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    acc
}
