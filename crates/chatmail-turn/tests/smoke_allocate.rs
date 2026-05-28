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

//! TURN Allocate with TURN REST credentials against embedded webrtc TURN.

mod support;

use std::net::SocketAddr;
use std::time::Duration;

use chatmail_turn::{
    parse_turn_metadata, spawn_turn_server_with_opts, turn_metadata_line, TurnSpawnOpts,
};
use support::turn_allocate;

#[tokio::test]
async fn turn_smoke_turn_allocate() {
    let secret = "allocate-smoke-secret";
    let realm = "test";
    let listen: SocketAddr = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap()
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let line = turn_metadata_line("127.0.0.1", listen.port(), secret, 3600, now).unwrap();
    let parsed = parse_turn_metadata(&line).unwrap();

    let _server = spawn_turn_server_with_opts(secret, realm, listen, listen, TurnSpawnOpts::for_tests())
        .await
        .expect("spawn TURN");
    tokio::time::sleep(Duration::from_millis(300)).await;

    let relay = turn_allocate(
        listen,
        secret,
        realm,
        &parsed.expiration_timestamp.to_string(),
        None,
    )
    .await
    .expect("TURN Allocate with REST credentials");

    assert_ne!(
        relay.port(),
        listen.port(),
        "relay port must differ from TURN control port, got {}",
        relay.port()
    );
}

/// Optional coturn integration when `COTURN_UCLIENT_PATH` is set (external binary, not in-tree).
#[tokio::test]
async fn turn_smoke_turn_allocate_coturn_optional() -> anyhow::Result<()> {
    let path = match std::env::var("COTURN_UCLIENT_PATH") {
        Ok(p) if !p.is_empty() => p,
        _ => return Ok(()),
    };

    let secret = "static_auth_secret";
    let _listen: SocketAddr = "127.0.0.1:3478".parse().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    let username = (now + 3600).to_string();
    let password = chatmail_turn::hmac_turn_password(secret, &username)?;

    let output = tokio::process::Command::new(&path)
        .args([
            "-L",
            "127.0.0.1",
            "-e",
            "127.0.0.1",
            "-u",
            &username,
            "-w",
            &password,
            "-X",
            "-y",
            "127.0.0.1",
        ])
        .output()
        .await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("ERROR"),
        "coturn uclient failed:\n{stdout}"
    );
    assert!(
        stdout.contains("Total lost packets 0 (0.000000%)"),
        "coturn uclient:\n{stdout}"
    );
    Ok(())
}
