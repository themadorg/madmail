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

//! Live HTTP scrape against `run_openmetrics_listener`.

use std::net::SocketAddr;
use std::time::Duration;

use chatmail_metrics::{
    record_smtp_completed, record_smtp_started, run_openmetrics_listener, set_queue_length,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

async fn tcp_get_metrics(addr: SocketAddr) -> String {
    let mut stream = TcpStream::connect(addr).await.expect("tcp connect");
    stream
        .write_all(
            b"GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        )
        .await
        .expect("write");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    String::from_utf8_lossy(&raw).into_owned()
}

fn reserve_listen_addr() -> String {
    let s = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    s.local_addr().expect("addr").to_string()
}

fn parse_sample(body: &str, name: &str, labels: &str) -> Option<f64> {
    for line in body.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if !line.starts_with(name) {
            continue;
        }
        if !labels.is_empty() && !line.contains(labels) {
            continue;
        }
        return line.split_whitespace().last()?.parse().ok();
    }
    None
}

#[tokio::test]
async fn openmetrics_serves_prometheus_text_on_metrics_path() {
    let listen = reserve_listen_addr();
    let url = format!("http://{listen}/metrics");
    let cancel = CancellationToken::new();
    let cancel_bg = cancel.clone();
    let listen_bg = listen.clone();

    let server = tokio::spawn(async move {
        run_openmetrics_listener(&listen_bg, cancel_bg).await
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    if server.is_finished() {
        match server.await.expect("server join") {
            Ok(()) => panic!("openmetrics server exited early without cancel"),
            Err(e) => panic!("openmetrics server failed: {e}"),
        }
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    let addr: SocketAddr = listen.parse().expect("listen addr");
    let raw = tcp_get_metrics(addr).await;
    assert!(
        raw.contains("HTTP/1.1 200") || raw.contains("HTTP/1.0 200"),
        "expected 200 in raw response: {}",
        &raw[..raw.len().min(400)]
    );
    assert!(
        raw.contains("maddy_smtp_started_transactions")
            || raw.contains("# HELP maddy_smtp_started_transactions"),
        "expected prometheus text over TCP from {url}, raw: {}",
        &raw[..raw.len().min(800)]
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url}: {e}"));
    assert_eq!(resp.status(), 200, "metrics scrape");
    let body = resp.text().await.expect("metrics body");
    assert!(
        body.contains("maddy_smtp"),
        "reqwest body empty or missing metrics; len={} raw_prefix={}",
        body.len(),
        &raw[..raw.len().min(200)]
    );

    record_smtp_started("http_itest");
    record_smtp_completed("http_itest");
    set_queue_length("remote_queue", "/var/queue", 7.0);

    let resp = client.get(&url).send().await.expect("second scrape");
    assert_eq!(resp.status(), 200);
    let body2 = resp.text().await.expect("body");
    let started = parse_sample(&body2, "maddy_smtp_started_transactions", r#"module="http_itest""#)
        .expect("started sample");
    assert!(started >= 1.0, "body: {body2}");
    let completed =
        parse_sample(&body2, "maddy_smtp_smtp_completed_transactions", r#"module="http_itest""#)
            .expect("completed sample");
    assert!(completed >= 1.0);
    let queue = parse_sample(&body2, "maddy_queue_length", r#"module="remote_queue""#)
        .expect("queue sample");
    assert!((queue - 7.0).abs() < f64::EPSILON);

    let not_found = client
        .get(format!("http://{listen}/nope"))
        .send()
        .await
        .expect("404 request");
    assert_eq!(not_found.status(), 404);

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .expect("server join timeout")
        .expect("server task");
}
