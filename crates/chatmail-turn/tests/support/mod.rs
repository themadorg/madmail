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

//! Shared helpers for `chatmail-turn` integration tests.

#![allow(dead_code)]

use std::net::SocketAddr;

/// TURN Allocate; `realm_override` replaces the configured realm when set.
pub async fn turn_allocate(
    server: SocketAddr,
    secret: &str,
    realm: &str,
    username: &str,
    realm_override: Option<&str>,
) -> Result<SocketAddr, String> {
    chatmail_turn::turn_allocate_on_socket(server, secret, realm, username, realm_override).await
}

pub const STUN_BINDING_REQUEST: &[u8] = &[
    0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xA4, 0x42, 0x45, 0x58, 0x65, 0x61, 0x57, 0x53, 0x5A,
    0x6E, 0x57, 0x35, 0x76, 0x46,
];

pub async fn exchange(
    socket: &tokio::net::UdpSocket,
    server: SocketAddr,
    payload: &[u8],
) -> Result<(Vec<u8>, SocketAddr), String> {
    socket
        .send_to(payload, server)
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = [0u8; 2048];
    let (n, from) = tokio::time::timeout(std::time::Duration::from_secs(3), socket.recv_from(&mut buf))
        .await
        .map_err(|_| "recv timeout".to_string())?
        .map_err(|e| e.to_string())?;
    Ok((buf[..n].to_vec(), from))
}
