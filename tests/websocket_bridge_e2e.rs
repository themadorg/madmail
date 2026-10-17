//! Native-protocol Rust client used both in-process and against an isolated Docker relay.
mod support;

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use chatmail_db::{set_setting, settings_keys};
use chatmail_www::bridge::BridgeSessions;
use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::ServerName;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
use tokio_tungstenite::{client_async, WebSocketStream};
use tokio_util::sync::CancellationToken;

type Socket = WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>;
const USER: &str = "bridgeuser@test";
const PASS: &str = "bridge-secret";

fn tls_config(pem: &str) -> Arc<rustls::ClientConfig> {
    let der = pem
        .lines()
        .filter(|l| !l.starts_with("---"))
        .collect::<String>();
    let der = base64::engine::general_purpose::STANDARD
        .decode(der)
        .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(der))
        .unwrap();
    Arc::new(
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

async fn connect(
    addr: &str,
    path: &str,
    tls: Arc<rustls::ClientConfig>,
    origin: Option<&str>,
) -> Result<Socket, tokio_tungstenite::tungstenite::Error> {
    let tcp = TcpStream::connect(addr).await.unwrap();
    let stream = TlsConnector::from(tls)
        .connect(ServerName::try_from("localhost").unwrap(), tcp)
        .await
        .unwrap();
    let mut req = format!("wss://localhost{path}")
        .into_client_request()
        .unwrap();
    if let Some(origin) = origin {
        req.headers_mut().insert("Origin", origin.parse().unwrap());
    }
    client_async(req, stream).await.map(|(ws, _)| ws)
}

struct MailClient {
    ws: Socket,
    buffered: Vec<u8>,
}
impl MailClient {
    async fn open(addr: &str, path: &str, tls: Arc<rustls::ClientConfig>) -> Self {
        Self {
            ws: connect(addr, path, tls, None).await.unwrap(),
            buffered: Vec::new(),
        }
    }
    async fn send(&mut self, bytes: &[u8]) {
        self.ws
            .send(Message::Binary(bytes.to_vec().into()))
            .await
            .unwrap();
    }
    async fn until(&mut self, needle: &[u8]) -> Vec<u8> {
        timeout(Duration::from_secs(15), async {
            loop {
                if let Some(pos) = self
                    .buffered
                    .windows(needle.len())
                    .position(|w| w == needle)
                {
                    return self.buffered.drain(..pos + needle.len()).collect();
                }
                match self
                    .ws
                    .next()
                    .await
                    .expect("socket closed")
                    .expect("WebSocket read")
                {
                    Message::Binary(data) => self.buffered.extend_from_slice(&data),
                    Message::Ping(_) => self.ws.flush().await.unwrap(),
                    other => panic!("unexpected message: {other:?}"),
                }
            }
        })
        .await
        .expect("mail response timeout")
    }
    async fn command(&mut self, command: &str, end: &[u8]) -> Vec<u8> {
        // Deliberately split commands in the middle of tokens and CRLF.
        let bytes = command.as_bytes();
        for chunk in bytes.chunks(3) {
            self.send(chunk).await;
        }
        self.until(end).await
    }
}

async fn exercise(addr: &str, tls: Arc<rustls::ClientConfig>) {
    let mut imap = MailClient::open(addr, "/transport/imap", tls.clone()).await;
    imap.until(b"ready\r\n").await;
    let denied = imap
        .command("a LOGIN bridgeuser@test wrong\r\n", b"\r\n")
        .await;
    assert!(String::from_utf8_lossy(&denied).starts_with("a NO"));
    let login = imap
        .command(
            &format!("b LOGIN {USER} {PASS}\r\n"),
            b"b OK LOGIN completed\r\n",
        )
        .await;
    assert!(!login.is_empty());
    imap.command("c SELECT INBOX\r\n", b"c OK [SELECT] completed\r\n")
        .await;
    imap.command("d IDLE\r\n", b"\r\n").await;
    imap.ws
        .send(Message::Ping(vec![1, 2, 3].into()))
        .await
        .unwrap();
    // Ping must not enter the mail byte stream; a Pong is emitted without mail activity.
    assert!(matches!(
        timeout(Duration::from_secs(5), imap.ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        Message::Pong(_)
    ));

    let mut smtp = MailClient::open(addr, "/transport/smtp", tls.clone()).await;
    smtp.until(b"\r\n").await;
    smtp.command("EHLO client\r\n", b"250 OK\r\n").await;
    let rejected = smtp
        .command(&format!("MAIL FROM:<{USER}>\r\n"), b"\r\n")
        .await;
    assert!(String::from_utf8_lossy(&rejected).contains("530"));
    let wrong = base64::engine::general_purpose::STANDARD.encode(format!("\0{USER}\0wrong"));
    let denied = smtp
        .command(&format!("AUTH PLAIN {wrong}\r\n"), b"\r\n")
        .await;
    assert!(String::from_utf8_lossy(&denied).starts_with("535"));
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("\0{USER}\0{PASS}"));
    smtp.command(&format!("AUTH PLAIN {auth}\r\n"), b"235 ")
        .await;
    smtp.until(b"\r\n").await;
    let spoof = smtp.command("MAIL FROM:<spoof@test>\r\n", b"\r\n").await;
    assert!(String::from_utf8_lossy(&spoof).starts_with("553"));
    smtp.command("RSET\r\n", b"\r\n").await;
    // Plaintext submission must be rejected by the normal PGP policy.
    smtp.command(&format!("MAIL FROM:<{USER}>\r\n"), b"\r\n")
        .await;
    smtp.command(&format!("RCPT TO:<{USER}>\r\n"), b"\r\n")
        .await;
    smtp.command("DATA\r\n", b"\r\n").await;
    smtp.send(
        format!("From: {USER}\r\nTo: {USER}\r\nSubject: plaintext\r\n\r\nnot encrypted\r\n.\r\n")
            .as_bytes(),
    )
    .await;
    let rejected = smtp.until(b"\r\n").await;
    assert!(
        String::from_utf8_lossy(&rejected).contains("523"),
        "{rejected:?}"
    );

    let message = String::from_utf8(support::pgp_mime_for_user(USER))
        .unwrap()
        .replace("stub", "stub\r\n.dot\r\n..double");
    smtp.command(&format!("MAIL FROM:<{USER}>\r\n"), b"\r\n")
        .await;
    smtp.command(&format!("RCPT TO:<{USER}>\r\n"), b"\r\n")
        .await;
    smtp.command("DATA\r\n", b"\r\n").await;
    let data = format!("{}\r\n.\r\n", message.trim_end().replace("\r\n.", "\r\n.."));
    for chunk in data.as_bytes().chunks(101) {
        smtp.send(chunk).await;
    }
    let accepted = smtp.until(b"\r\n").await;
    assert!(
        String::from_utf8_lossy(&accepted).starts_with("250"),
        "{accepted:?}"
    );
    imap.until(b"EXISTS\r\n").await;
    imap.command("DONE\r\n", b"d OK IDLE terminated\r\n").await;
    let fetched = imap
        .command("e FETCH 1 BODY.PEEK[]\r\n", b"e OK FETCH completed\r\n")
        .await;
    assert!(fetched
        .windows(b"application/pgp-encrypted".len())
        .any(|w| w == b"application/pgp-encrypted"));
    assert!(fetched
        .windows(b".dot\r\n..double".len())
        .any(|w| w == b".dot\r\n..double"));
    // A literal larger than the frame limit, including NUL and invalid UTF-8.
    let marker = message.find("stub").unwrap();
    let mut literal = message.as_bytes()[..marker].to_vec();
    for _ in 0..20000 {
        literal.extend_from_slice(b"\x00\xff\x80payload\r\n");
    }
    literal.extend_from_slice(&message.as_bytes()[marker + 4..]);
    imap.command(
        &format!("f APPEND INBOX {{{}}}\r\n", literal.len()),
        b"\r\n",
    )
    .await;
    for chunk in literal.chunks(32749) {
        imap.send(chunk).await;
    }
    imap.send(b"\r\n").await;
    imap.until(b"f OK APPEND completed\r\n").await;
    let fetched = imap
        .command("f2 FETCH 2 BODY.PEEK[]\r\n", b"f2 OK FETCH completed\r\n")
        .await;
    assert!(
        fetched
            .windows(literal.len())
            .any(|window| window == literal),
        "literal changed in transit"
    );
    imap.send(b"g NOOP\r\nh NOOP\r\n").await;
    imap.until(b"h OK NOOP completed\r\n").await;
    smtp.command("QUIT\r\n", b"221 ").await;
    imap.command("i LOGOUT\r\n", b"i OK LOGOUT completed\r\n")
        .await;

    let mut invalid = MailClient::open(addr, "/transport/imap", tls.clone()).await;
    invalid.until(b"ready\r\n").await;
    invalid
        .ws
        .send(Message::Text("a NOOP\r\n".into()))
        .await
        .unwrap();
    let closed = timeout(Duration::from_secs(5), invalid.ws.next())
        .await
        .unwrap();
    assert!(!matches!(closed, Some(Ok(Message::Binary(_)))));
    assert!(connect(
        addr,
        "/transport/imap?target=example.org:25",
        tls.clone(),
        None
    )
    .await
    .is_err());
    assert!(connect(
        addr,
        "/transport/imap",
        tls.clone(),
        Some("https://evil.example")
    )
    .await
    .is_err());
    let mut allowed = connect(addr, "/transport/imap", tls, Some("https://client.example"))
        .await
        .unwrap();
    allowed.close(None).await.unwrap();
}

#[tokio::test]
async fn websocket_bridge_native_protocols() {
    let dir = tempfile::tempdir().unwrap();
    let native = support::spawn_mail_servers(dir.path()).await;
    support::create_user(&native.ctx, &native.pool, USER, PASS).await;
    let cfg = chatmail_config::AppConfig {
        hostname: Some("localhost".into()),
        primary_domain: Some("test".into()),
        ..Default::default()
    };
    let policy = cfg.credential_policy();
    let sessions = BridgeSessions {
        imap: chatmail_imap::ImapSessionConfig {
            hostname: "localhost".into(),
            primary_domain: "test".into(),
            jit_domain: None,
            credential_policy: policy,
            turn: None,
            iroh: None,
            push_enabled: false,
            starttls_config: None,
        },
        submission: chatmail_smtp::SmtpSessionConfig {
            hostname: "localhost".into(),
            primary_domain: "test".into(),
            local_domains: vec!["test".into()],
            jit_domain: None,
            credential_policy: policy,
            require_auth: true,
            module: "submission",
            starttls_config: None,
        },
    };
    let cancel = CancellationToken::new();
    let router = chatmail_www::www_router(chatmail_www::WwwState::new(
        native.pool.clone(),
        native.ctx.clone(),
        cfg,
        dir.path(),
    ))
    .layer(axum::Extension(sessions))
    .layer(axum::Extension(cancel.clone()));
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let tls = tls_config(&cert.cert.pem());
    let server = Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.cert.der().clone()],
                rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()).into(),
            )
            .unwrap(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let task = tokio::spawn(async move {
        loop {
            let (tcp, peer) = listener.accept().await.unwrap();
            let acceptor = tokio_rustls::TlsAcceptor::from(server.clone());
            let router = router.clone();
            tokio::spawn(async move {
                if let Ok(tls) = acceptor.accept(tcp).await {
                    chatmail_fed::server::serve_tls_conn(tls, router, peer).await;
                }
            });
        }
    });
    assert!(connect(&addr, "/transport/imap", tls.clone(), None)
        .await
        .is_err());
    set_setting(
        &native.pool,
        settings_keys::WEBSOCKET_BRIDGE_ENABLED,
        "true",
    )
    .await
    .unwrap();
    set_setting(
        &native.pool,
        settings_keys::WEBMAIL_CORS_ORIGINS,
        "https://client.example",
    )
    .await
    .unwrap();
    native.ctx.auth.block(USER);
    let mut blocked = MailClient::open(&addr, "/transport/imap", tls.clone()).await;
    blocked.until(b"ready\r\n").await;
    let denied = blocked
        .command(&format!("a LOGIN {USER} {PASS}\r\n"), b"\r\n")
        .await;
    assert!(String::from_utf8_lossy(&denied).starts_with("a NO"));
    blocked.ws.close(None).await.unwrap();
    native.ctx.auth.unblock(USER);
    // Even forged proxy headers cannot turn clear HTTP into a trusted transport.
    let tcp = TcpStream::connect(native.http_addr).await.unwrap();
    let mut req = "ws://localhost/transport/imap"
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("X-Forwarded-Proto", "https".parse().unwrap());
    match client_async(req, tcp).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(res)) => assert_eq!(res.status(), 403),
        _ => panic!("plaintext bridge accepted"),
    }
    exercise(&addr, tls.clone()).await;
    // A message above the transport limit closes the connection, without entering IMAP.
    let mut oversized = MailClient::open(&addr, "/transport/imap", tls.clone()).await;
    oversized.until(b"ready\r\n").await;
    let _ = oversized
        .ws
        .send(Message::Binary(vec![b'x'; 65537].into()))
        .await;
    assert!(!matches!(
        timeout(Duration::from_secs(5), oversized.ws.next())
            .await
            .unwrap(),
        Some(Ok(Message::Binary(_)))
    ));
    set_setting(
        &native.pool,
        settings_keys::WEBSOCKET_BRIDGE_ENABLED,
        "false",
    )
    .await
    .unwrap();
    match connect(&addr, "/transport/imap", tls.clone(), None).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(res)) => assert_eq!(res.status(), 404),
        _ => panic!("disabled override ignored"),
    }
    set_setting(
        &native.pool,
        settings_keys::WEBSOCKET_BRIDGE_ENABLED,
        "true",
    )
    .await
    .unwrap();
    let mut client = support::ImapClient::connect(native.imap_addr).await;
    client.command(&format!("a LOGIN {USER} {PASS}")).await;
    let status = client.command("b STATUS INBOX (MESSAGES)").await;
    assert!(status.contains("MESSAGES 2"), "{status}");
    let mut active = MailClient::open(&addr, "/transport/imap", tls).await;
    active.until(b"ready\r\n").await;
    active
        .command(
            &format!("a LOGIN {USER} {PASS}\r\n"),
            b"a OK LOGIN completed\r\n",
        )
        .await;
    let mut silent = MailClient::open(&addr, "/transport/imap", tls_config(&cert.cert.pem())).await;
    silent.until(b"ready\r\n").await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(61)).await;
    assert!(!matches!(
        timeout(Duration::from_secs(5), silent.ws.next())
            .await
            .unwrap(),
        Some(Ok(Message::Binary(_)))
    ));
    tokio::time::resume();
    active
        .command("j NOOP\r\n", b"j OK NOOP completed\r\n")
        .await;
    // The process-wide cap includes every upgraded session, and closing releases it.
    let cap_tls = tls_config(&cert.cert.pem());
    let mut held = Vec::new();
    for _ in 0..255 {
        let mut connection = MailClient::open(&addr, "/transport/imap", cap_tls.clone()).await;
        connection.until(b"ready\r\n").await;
        held.push(connection);
    }
    match connect(&addr, "/transport/imap", cap_tls.clone(), None).await {
        Err(tokio_tungstenite::tungstenite::Error::Http(res)) => assert_eq!(res.status(), 503),
        _ => panic!("bridge connection limit not enforced"),
    }
    drop(held);
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(mut ws) = connect(&addr, "/transport/imap", cap_tls.clone(), None).await {
                let _ = ws.close(None).await;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    assert!(!matches!(
        timeout(Duration::from_secs(5), active.ws.next())
            .await
            .unwrap(),
        Some(Ok(Message::Binary(_)))
    ));
    task.abort();
}

#[tokio::test]
#[ignore = "run tests/websocket-bridge-docker.sh"]
async fn docker_websocket_bridge() {
    let addr = std::env::var("BRIDGE_TEST_ADDR").unwrap();
    let pem = std::fs::read_to_string(std::env::var("BRIDGE_TEST_CA").unwrap()).unwrap();
    let tls = tls_config(&pem);
    if std::env::var("BRIDGE_TEST_MODE").as_deref() == Ok("disabled") {
        for path in ["/transport/imap", "/transport/smtp"] {
            match connect(&addr, path, tls.clone(), None).await {
                Err(tokio_tungstenite::tungstenite::Error::Http(res)) => {
                    assert_eq!(res.status(), 404)
                }
                _ => panic!("disabled service must return 404"),
            }
        }
    } else {
        exercise(&addr, tls).await;
    }
}
