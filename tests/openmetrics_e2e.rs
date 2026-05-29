//! End-to-end: SMTP session increments counters exposed on `/metrics`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use chatmail_auth::hash_password;
use chatmail_metrics::run_openmetrics_listener;
use chatmail_smtp::session::PGP_MIME_BODY;
use chatmail_smtp::{SmtpSession, SmtpSessionConfig};
use chatmail_state::AppState;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

fn reserve_addr() -> SocketAddr {
    let s = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    s.local_addr().expect("addr")
}

async fn scrape_metrics(url: &str) -> String {
    reqwest::Client::new()
        .get(url)
        .send()
        .await
        .expect("metrics GET")
        .error_for_status()
        .expect("metrics 200")
        .text()
        .await
        .expect("metrics body")
}

fn sample(body: &str, name: &str, labels: &str) -> f64 {
    for line in body.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with(name) && line.contains(labels) {
            if let Some(v) = line.split_whitespace().last() {
                return v.parse().unwrap_or(0.0);
            }
        }
    }
    0.0
}

async fn smtp_submit_encrypted(smtp_addr: SocketAddr) {
    let b64 = base64::engine::general_purpose::STANDARD.encode("\0u@test\0secret");
    let auth = format!("AUTH PLAIN {b64}");
    let body = std::str::from_utf8(PGP_MIME_BODY)
        .unwrap()
        .replace("sender@test", "u@test")
        .replace("rcpt@test", "u@test");

    tokio::time::sleep(Duration::from_millis(30)).await;
    let mut stream = TcpStream::connect(smtp_addr).await.expect("smtp connect");
    let mut buf = [0u8; 4096];

    async fn expect_contains(stream: &mut TcpStream, buf: &mut [u8; 4096], needle: &str) {
        let mut acc = String::new();
        for _ in 0..20 {
            let n = stream.read(buf).await.unwrap_or(0);
            if n > 0 {
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            if acc.contains(needle) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("expected {needle} in {acc}");
    }

    async fn cmd(stream: &mut TcpStream, buf: &mut [u8; 4096], line: &str, needle: &str) {
        stream
            .write_all(format!("{line}\r\n").as_bytes())
            .await
            .expect("write");
        expect_contains(stream, buf, needle).await;
    }

    expect_contains(&mut stream, &mut buf, "220").await;
    cmd(&mut stream, &mut buf, "EHLO client.test", "250").await;
    cmd(&mut stream, &mut buf, &auth, "235").await;
    cmd(&mut stream, &mut buf, "MAIL FROM:<u@test>", "250").await;
    cmd(&mut stream, &mut buf, "RCPT TO:<u@test>", "250").await;
    cmd(&mut stream, &mut buf, "DATA", "354").await;
    for line in body.lines() {
        if line.is_empty() {
            continue;
        }
        stream
            .write_all(format!("{line}\r\n").as_bytes())
            .await
            .expect("data line");
    }
    stream.write_all(b".\r\n").await.expect("data end");
    expect_contains(&mut stream, &mut buf, "250 2.0.0").await;
    stream.write_all(b"QUIT\r\n").await.ok();
}

#[tokio::test]
async fn smtp_submission_increments_openmetrics_counters() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = chatmail_db::init_memory_db().await.expect("db");
    let hash = hash_password("secret").expect("hash");
    chatmail_db::passwords::create_user(&pool, "u@test", &hash)
        .await
        .expect("user");
    let ctx = Arc::new(AppState::new(dir.path()));
    ctx.auth.hydrate(&pool).await.expect("auth hydrate");

    let metrics_addr = reserve_addr();
    let metrics_url = format!("http://{metrics_addr}/metrics");
    let smtp_addr = reserve_addr();

    let cancel = CancellationToken::new();
    let metrics_listen = metrics_addr.to_string();
    let cancel_bg = cancel.clone();
    let metrics_task = tokio::spawn(async move {
        run_openmetrics_listener(&metrics_listen, cancel_bg)
            .await
            .expect("openmetrics")
    });

    tokio::time::sleep(Duration::from_millis(80)).await;

    let label = r#"module="submission""#;
    let before_started = sample(
        &scrape_metrics(&metrics_url).await,
        "maddy_smtp_started_transactions",
        label,
    );
    let before_completed = sample(
        &scrape_metrics(&metrics_url).await,
        "maddy_smtp_smtp_completed_transactions",
        label,
    );

    let std_listener = std::net::TcpListener::bind(smtp_addr).expect("smtp bind");
    std_listener.set_nonblocking(true).expect("nb");
    let smtp_task = tokio::spawn(async move {
        let listener = TcpListener::from_std(std_listener).expect("tokio smtp");
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let pool = pool.clone();
            let ctx = Arc::clone(&ctx);
            tokio::spawn(async move {
                let mut session = SmtpSession::new(
                    ctx,
                    pool,
                    SmtpSessionConfig {
                        hostname: "mx.test".into(),
                        primary_domain: "test".into(),
                        local_domains: vec!["test".into()],
                        jit_domain: None,
                        credential_policy: chatmail_config::CredentialPolicy::default(),
                        require_auth: true,
                        module: "submission",
                    },
                );
                let _ = session.handle_connection(stream).await;
            });
        }
    });

    smtp_submit_encrypted(smtp_addr).await;
    smtp_task.abort();

    tokio::time::sleep(Duration::from_millis(50)).await;
    let body = scrape_metrics(&metrics_url).await;

    let after_started = sample(&body, "maddy_smtp_started_transactions", label);
    let after_completed = sample(&body, "maddy_smtp_smtp_completed_transactions", label);

    assert!(
        after_started > before_started,
        "started before={before_started} after={after_started}\n{body}"
    );
    assert!(
        after_completed > before_completed,
        "completed before={before_completed} after={after_completed}\n{body}"
    );

    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(3), metrics_task)
        .await
        .expect("metrics shutdown")
        .expect("metrics task");
}
