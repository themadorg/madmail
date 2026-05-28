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

//! Live checks against a running chatmail server (credentials via env only).
//!
//! ```bash
//! export CHATMAIL_TURN_REMOTE=1
//! export CHATMAIL_TURN_IMAP_HOST=mail.example.com:993
//! export CHATMAIL_TURN_IMAP_USER='user@mail.example.com'
//! export CHATMAIL_TURN_IMAP_PASS='…'
//! export CHATMAIL_TURN_SECRET='…'   # same as madmail turn_secret
//! export CHATMAIL_TURN_REALM=mail.example.com   # optional; defaults from metadata
//! cargo test -p chatmail-turn turn_remote_live -- --ignored --nocapture
//! ```

use std::net::SocketAddr;
use std::time::Duration;

use chatmail_turn::{parse_turn_metadata, TurnClient};

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

fn extract_turn_line(imap_response: &str) -> String {
    let marker = "/shared/vendor/deltachat/turn";
    let idx = imap_response
        .find(marker)
        .unwrap_or_else(|| panic!("turn key missing: {imap_response}"));
    let after = &imap_response[idx + marker.len()..];
    let start = after.find('"').expect("quote") + 1;
    let rest = &after[start..];
    let end = rest.find('"').expect("close quote");
    rest[..end].to_string()
}

async fn imap_dialog(host_port: &str, user: &str, pass: &str, tag_cmd: &str) -> String {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(host_port).await.expect("tcp imap");
    let (reader, mut writer) = stream.split();
    let mut lines = tokio::io::BufReader::new(reader).lines();
    let _greeting = lines.next_line().await.expect("greeting");

    writer
        .write_all(format!("a001 LOGIN {user} {pass}\r\n").as_bytes())
        .await
        .expect("login");
    let mut login_resp = String::new();
    while let Some(line) = lines.next_line().await.expect("login lines") {
        login_resp.push_str(&line);
        login_resp.push('\n');
        if line.starts_with("a001 ") {
            break;
        }
    }
    assert!(
        login_resp.contains("a001 OK"),
        "LOGIN failed:\n{login_resp}"
    );

    writer
        .write_all(format!("{tag_cmd}\r\n").as_bytes())
        .await
        .expect("cmd");
    let mut out = String::new();
    let tag = tag_cmd.split_whitespace().next().unwrap();
    while let Some(line) = lines.next_line().await.expect("cmd lines") {
        out.push_str(&line);
        out.push('\n');
        if line.starts_with(tag) {
            break;
        }
    }
    out
}

#[tokio::test]
#[ignore = "requires CHATMAIL_TURN_REMOTE=1 and IMAP/TURN env (see module doc)"]
async fn turn_remote_live_imap_metadata_and_relay() {
    if env("CHATMAIL_TURN_REMOTE") != Some("1".into()) {
        return;
    }
    let host = env("CHATMAIL_TURN_IMAP_HOST").expect("CHATMAIL_TURN_IMAP_HOST");
    let user = env("CHATMAIL_TURN_IMAP_USER").expect("CHATMAIL_TURN_IMAP_USER");
    let pass = env("CHATMAIL_TURN_IMAP_PASS").expect("CHATMAIL_TURN_IMAP_PASS");
    let secret = env("CHATMAIL_TURN_SECRET").expect("CHATMAIL_TURN_SECRET for relay datapath");

    let caps = imap_dialog(&host, &user, &pass, "a002 CAPABILITY").await;
    assert!(
        caps.contains("METADATA"),
        "server must advertise METADATA: {caps}"
    );

    let meta = imap_dialog(
        &host,
        &user,
        &pass,
        "a003 GETMETADATA \"\" (/shared/vendor/deltachat/turn /shared/vendor/deltachat/turn-test-relay-only)",
    )
    .await;
    assert!(meta.contains("a003 OK"), "GETMETADATA failed:\n{meta}");
    let line = extract_turn_line(&meta);
    let parsed = parse_turn_metadata(&line).expect("parse turn line");
    assert!(parsed.port > 0, "invalid turn metadata: {line}");
    let realm = env("CHATMAIL_TURN_REALM").unwrap_or_else(|| parsed.hostname.clone());

    let turn_server: SocketAddr = format!("{}:{}", parsed.hostname, parsed.port)
        .parse()
        .unwrap_or_else(|_| panic!("TURN addr {}:{}", parsed.hostname, parsed.port));

    let username = parsed.expiration_timestamp.to_string();
    let mut alice = TurnClient::new(turn_server, &secret, &realm, &username)
        .await
        .expect("alice");
    let mut bob = TurnClient::new(
        turn_server,
        &secret,
        &realm,
        &(parsed.expiration_timestamp + 7200).to_string(),
    )
    .await
    .expect("bob");

    let ra = alice.allocate().await.expect("allocate alice");
    let rb = bob.allocate().await.expect("allocate bob");
    assert!(ra.port() >= 49152 && rb.port() >= 49152);

    alice.create_permission(rb).await.expect("perm a→b");
    bob.create_permission(ra).await.expect("perm b→a");
    alice.send(rb, b"live-relay").await.expect("send");
    let (_, data) = bob.recv_data(Duration::from_secs(5)).await.expect("recv");
    assert_eq!(data, b"live-relay");
}
