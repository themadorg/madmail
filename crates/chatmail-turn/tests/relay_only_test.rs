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

//! Relay-only metadata mode: Allocate still works (STUN on :3478 is not rejected by webrtc turn).

mod support;

use std::net::SocketAddr;
use std::time::Duration;

use chatmail_turn::{spawn_turn_server_with_opts, TurnSpawnOpts};
use support::turn_allocate;

#[tokio::test]
async fn turn_test_relay_only_still_allocates() {
    let secret = "relay-only-secret";
    let realm = "test";
    let listen: SocketAddr = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap()
    };
    let _srv = spawn_turn_server_with_opts(
        secret,
        realm,
        listen,
        listen,
        TurnSpawnOpts {
            test_relay_only: true,
            ..TurnSpawnOpts::for_tests()
        },
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let username = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 3600)
        .to_string();
    let relay = turn_allocate(listen, secret, realm, &username, None)
        .await
        .expect("TURN Allocate must work in relay-only test mode");
    assert_ne!(relay.port(), listen.port());
}
