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

//! Minimal TURN client for tests ([RFC 8656] via webrtc-rs `turn` 0.11).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UdpSocket;
use turn::client::{Client, ClientConfig};
use webrtc_util::Conn;

use crate::credentials::hmac_turn_password;

async fn build_client(
    server: SocketAddr,
    secret: &str,
    realm: &str,
    username: &str,
) -> Result<Client, String> {
    let password = hmac_turn_password(secret, username).map_err(|e| e.to_string())?;
    let server_addr = server.to_string();
    let conn = UdpSocket::bind("127.0.0.1:0").await.map_err(|e| e.to_string())?;
    let client = Client::new(ClientConfig {
        stun_serv_addr: server_addr.clone(),
        turn_serv_addr: server_addr,
        username: username.to_string(),
        password,
        realm: realm.to_string(),
        software: String::new(),
        rto_in_ms: 500,
        conn: Arc::new(conn),
        vnet: None,
    })
    .await
    .map_err(|e| e.to_string())?;
    client.listen().await.map_err(|e| e.to_string())?;
    Ok(client)
}

/// UDP TURN session (one allocation).
pub struct TurnClient {
    #[allow(dead_code)]
    client: Client,
    relay: Option<Box<dyn Conn + Send + Sync>>,
}

impl TurnClient {
    pub async fn new(
        server: SocketAddr,
        secret: impl AsRef<str>,
        realm: impl AsRef<str>,
        username: impl AsRef<str>,
    ) -> Result<Self, String> {
        let client = build_client(
            server,
            secret.as_ref(),
            realm.as_ref(),
            username.as_ref(),
        )
        .await?;
        Ok(Self {
            client,
            relay: None,
        })
    }

    /// [RFC 8656] §6 — TURN Allocate (TURN REST credentials).
    pub async fn allocate(&mut self) -> Result<SocketAddr, String> {
        let relay = self.client.allocate().await.map_err(|e| e.to_string())?;
        let addr = relay.local_addr().map_err(|e| e.to_string())?;
        self.relay = Some(Box::new(relay));
        Ok(addr)
    }

    pub fn relay(&self) -> Option<SocketAddr> {
        self.relay
            .as_ref()
            .and_then(|r| r.local_addr().ok())
    }

    /// [RFC 8656] §9 — open permission for `peer_relay` (webrtc turn: first `send_to` on that peer).
    pub async fn create_permission(&self, peer_relay: SocketAddr) -> Result<(), String> {
        let relay = self.relay.as_ref().ok_or("allocate first")?;
        relay
            .send_to(&[0], peer_relay)
            .await
            .map_err(|e| e.to_string())?;
        tokio::time::sleep(Duration::from_millis(150)).await;
        Ok(())
    }

    /// [RFC 8656] §12 — send payload to peer via relay.
    pub async fn send(&self, peer: SocketAddr, data: &[u8]) -> Result<(), String> {
        let relay = self.relay.as_ref().ok_or("allocate first")?;
        relay
            .send_to(data, peer)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Wait for inbound data on the relay socket.
    pub async fn recv_data(&self, timeout: Duration) -> Result<(SocketAddr, Vec<u8>), String> {
        let relay = self.relay.as_ref().ok_or("allocate first")?;
        let mut buf = vec![0u8; 2048];
        let (n, from) = tokio::time::timeout(timeout, relay.recv_from(&mut buf))
            .await
            .map_err(|_| "recv timeout waiting for relay data".to_string())?
            .map_err(|e| e.to_string())?;
        Ok((from, buf[..n].to_vec()))
    }
}

/// One-shot Allocate.
pub async fn turn_allocate(
    server: SocketAddr,
    secret: &str,
    realm: &str,
    username: &str,
) -> Result<SocketAddr, String> {
    turn_allocate_on_socket(server, secret, realm, username, None).await
}

pub async fn turn_allocate_on_socket(
    server: SocketAddr,
    secret: &str,
    realm: &str,
    username: &str,
    realm_override: Option<&str>,
) -> Result<SocketAddr, String> {
    let use_realm = realm_override.unwrap_or(realm);
    let client = build_client(server, secret, use_realm, username).await?;
    let relay = client.allocate().await.map_err(|e| e.to_string())?;
    relay.local_addr().map_err(|e| e.to_string())
}
