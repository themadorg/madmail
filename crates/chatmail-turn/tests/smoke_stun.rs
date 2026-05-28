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

//! STUN Binding against embedded webrtc TURN.

mod support;

use std::net::SocketAddr;
use std::time::Duration;

use chatmail_turn::{spawn_turn_server_with_opts, TurnSpawnOpts};
use support::{exchange, STUN_BINDING_REQUEST};
use tokio::net::UdpSocket;

#[tokio::test]
async fn turn_smoke_stun_binding() {
    let listen: SocketAddr = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap()
    };

    let _srv = spawn_turn_server_with_opts("smoke-secret", "test", listen, listen, TurnSpawnOpts::for_tests())
        .await
        .expect("spawn TURN");
    tokio::time::sleep(Duration::from_millis(300)).await;

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .send_to(STUN_BINDING_REQUEST, listen)
        .await
        .expect("send binding");
    let (resp, _) = exchange(&socket, listen, STUN_BINDING_REQUEST)
        .await
        .expect("binding response");
    assert!(resp.len() >= 20, "short STUN response: {} bytes", resp.len());
    assert_eq!(
        &resp[4..8],
        &[0x21, 0x12, 0xA4, 0x42],
        "STUN magic cookie in response"
    );
}
