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

//! Two independent allocations (simulates caller + callee) must get distinct relay ports.

mod support;

use std::net::SocketAddr;
use std::time::Duration;

use chatmail_turn::{spawn_turn_server_with_opts, TurnSpawnOpts};
use support::turn_allocate;

#[tokio::test]
async fn turn_dual_allocate_distinct_ports() {
    let secret = "dual-secret";
    let realm = "dual";
    let listen: SocketAddr = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap()
    };
    let _srv = spawn_turn_server_with_opts(secret, realm, listen, listen, TurnSpawnOpts::for_tests())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let u1 = (now() + 3600).to_string();
    let u2 = (now() + 7200).to_string();
    let r1 = turn_allocate(listen, secret, realm, &u1, None)
        .await
        .expect("first allocate");
    let r2 = turn_allocate(listen, secret, realm, &u2, None)
        .await
        .expect("second allocate");
    assert_ne!(r1.port(), r2.port(), "two call legs need distinct relay ports");
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
