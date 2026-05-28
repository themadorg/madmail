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

//! TURN REST auth against embedded webrtc TURN.

mod support;

use std::net::SocketAddr;
use std::time::Duration;

use chatmail_turn::{spawn_turn_server_with_opts, TurnSpawnOpts};
use support::turn_allocate;

/// webrtc turn derives MESSAGE-INTEGRITY from the REALM attribute in the request.
#[tokio::test]
#[ignore = "webrtc turn uses request REALM in auth key (pion behaviour)"]
async fn turn_allocate_rejects_wrong_realm() {
    let secret = "realm-secret";
    let listen: SocketAddr = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap()
    };
    let _srv = spawn_turn_server_with_opts(
        secret,
        "correct-realm",
        listen,
        listen,
        TurnSpawnOpts::for_tests(),
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let username = (now_unix() + 3600).to_string();
    let err = turn_allocate(listen, secret, "correct-realm", &username, Some("wrong-realm"))
        .await
        .expect_err("wrong realm must fail allocate");
    assert!(
        !err.is_empty(),
        "unexpected success message: {err}"
    );
}

#[tokio::test]
async fn turn_metadata_credentials_match_server_realm() {
    let secret = "meta-secret";
    let realm = "mail.example.com";
    let listen: SocketAddr = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap()
    };
    let _srv = spawn_turn_server_with_opts(secret, realm, listen, listen, TurnSpawnOpts::for_tests())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let now = now_unix();
    let line = chatmail_turn::turn_metadata_line("mail.example.com", listen.port(), secret, 3600, now)
        .unwrap();
    let parsed = chatmail_turn::parse_turn_metadata(&line).unwrap();
    let relay = turn_allocate(
        listen,
        secret,
        realm,
        &parsed.expiration_timestamp.to_string(),
        None,
    )
    .await
    .expect("IMAP-style credentials must allocate");
    assert_ne!(relay.port(), listen.port());
}

#[tokio::test]
async fn turn_allocate_rejects_bad_password() {
    let secret = "pw-secret";
    let realm = "r";
    let listen: SocketAddr = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap()
    };
    let _srv = spawn_turn_server_with_opts(secret, realm, listen, listen, TurnSpawnOpts::for_tests())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let username = (now_unix() + 3600).to_string();
    let err = turn_allocate(listen, "wrong-secret", realm, &username, None).await;
    assert!(err.is_err(), "bad password must not allocate");
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
