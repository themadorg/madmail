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

//! [RFC 8656] relay datapath: Allocate → CreatePermission → Send/Data between two allocations.

use std::net::SocketAddr;
use std::time::Duration;

use chatmail_turn::{spawn_turn_server_with_opts, TurnClient, TurnSpawnOpts};

fn now_username(offset_secs: i64) -> String {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + offset_secs)
        .to_string()
}

#[tokio::test]
async fn turn_rfc8656_relay_send_indication_datapath() {
    let secret = "relay-datapath-secret";
    let realm = "relay.test";
    let listen: SocketAddr = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap()
    };
    let external = SocketAddr::new(listen.ip(), listen.port());

    let _srv = spawn_turn_server_with_opts(secret, realm, listen, external, TurnSpawnOpts::for_tests())
        .await
        .expect("spawn TURN");
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut alice = TurnClient::new(listen, secret, realm, now_username(3600))
        .await
        .expect("alice client");
    let mut bob = TurnClient::new(listen, secret, realm, now_username(7200))
        .await
        .expect("bob client");

    let relay_a = alice.allocate().await.expect("alice allocate");
    let relay_b = bob.allocate().await.expect("bob allocate");
    assert_ne!(relay_a.port(), relay_b.port(), "distinct relay ports");
    assert_ne!(relay_a.port(), listen.port(), "relay port is not TURN control port");
    assert_ne!(relay_b.port(), listen.port(), "relay port is not TURN control port");

    alice
        .create_permission(relay_b)
        .await
        .expect("alice permission for bob");
    bob.create_permission(relay_a)
        .await
        .expect("bob permission for alice");

    drain_probes(&bob).await;
    drain_probes(&alice).await;

    alice
        .send(relay_b, b"hello-via-relay")
        .await
        .expect("send to bob relay");
    let (from, _payload) = recv_until(&bob, Duration::from_secs(3), b"hello-via-relay")
        .await
        .expect("data at bob");
    assert_eq!(from.port(), relay_a.port(), "peer port is sender relay port");

    bob.send(relay_a, b"pong-via-relay")
        .await
        .expect("send to alice relay");
    let (from2, payload2) = recv_until(&alice, Duration::from_secs(3), b"pong-via-relay")
        .await
        .expect("data at alice");
    assert_eq!(payload2, b"pong-via-relay");
    assert_eq!(from2.port(), relay_b.port());
}

/// Drop zero-byte permission probes from `create_permission`.
async fn drain_probes(client: &TurnClient) {
    for _ in 0..4 {
        match client.recv_data(Duration::from_millis(150)).await {
            Ok((_, ref p)) if p.as_slice() == [0] => continue,
            _ => break,
        }
    }
}

async fn recv_until(
    client: &TurnClient,
    timeout: Duration,
    want: &[u8],
) -> Result<(SocketAddr, Vec<u8>), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let (from, payload) = client.recv_data(remaining.min(Duration::from_millis(500))).await?;
        if payload == want {
            return Ok((from, payload));
        }
    }
    Err(format!("timeout waiting for {:?}", std::str::from_utf8(want).unwrap_or("<bin>")))
}
