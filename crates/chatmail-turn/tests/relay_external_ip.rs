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

//! Relay XOR-RELAYED-ADDRESS must use configured external IP, not loopback listen.

mod support;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use chatmail_turn::{spawn_turn_server_with_opts, TurnSpawnOpts};
use support::turn_allocate;

const RELAY_PUBLIC: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 10);

#[tokio::test]
async fn turn_relay_advertises_external_ip() {
    let secret = "relay-ext-secret";
    let realm = "relay.test";
    let listen: SocketAddr = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap()
    };
    let external = SocketAddr::new(IpAddr::V4(RELAY_PUBLIC), listen.port());

    let _srv = spawn_turn_server_with_opts(secret, realm, listen, external, TurnSpawnOpts::for_tests())
        .await
        .expect("spawn");
    tokio::time::sleep(Duration::from_millis(300)).await;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 3600;
    let relay = turn_allocate(listen, secret, realm, &now.to_string(), None)
        .await
        .expect("allocate");

    assert_eq!(
        relay.ip(),
        IpAddr::V4(RELAY_PUBLIC),
        "clients must see the public relay IP in ICE candidates, not 127.0.0.1"
    );
    assert_ne!(relay.port(), listen.port(), "relay port must not be control port");
}
