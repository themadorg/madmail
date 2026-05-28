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

//! TURN REST credentials and webrtc-rs TURN server for Chatmail (Delta Chat calls).

mod allocate_client;
mod credentials;
mod parse;
mod runner;
mod turn_client;

pub use allocate_client::turn_allocate;
pub use turn_client::{turn_allocate_on_socket, TurnClient};

pub use credentials::{hmac_turn_password, turn_metadata_line, TurnCredentialError};
pub use parse::{parse_turn_metadata, ParsedTurnMetadata, ParseTurnMetadataError};
pub use runner::{
    spawn_turn_server, spawn_turn_server_with_opts, turn_debug_from_env,
    turn_force_relay_test_from_env, TurnServerHandle, TurnSpawnOpts,
};

/// Discovery settings advertised via IMAP METADATA ([RFC 5464]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnDiscovery {
    pub server: String,
    pub port: u16,
    pub secret: String,
    /// Credential lifetime added to `now` for the REST username (seconds).
    pub ttl_secs: u64,
    /// Advertise `/shared/vendor/deltachat/turn-test-relay-only` = "1" (Core: `iceTransportPolicy: relay`).
    pub turn_test_relay_only: bool,
}

impl TurnDiscovery {
    pub fn enabled(&self) -> bool {
        !self.secret.is_empty() && !self.server.is_empty()
    }

    /// Build from static `maddy.conf` / chatmail config fields.
    pub fn from_config(
        turn_enable: bool,
        server: String,
        port: u16,
        secret: Option<String>,
        ttl_secs: u64,
        turn_test_relay_only: bool,
    ) -> Option<Self> {
        let secret = secret.filter(|s| !s.is_empty())?;
        if !turn_enable {
            return None;
        }
        let port = if port == 0 { 3478 } else { port };
        let ttl_secs = if ttl_secs == 0 { 86400 } else { ttl_secs };
        Some(Self {
            server,
            port,
            secret,
            ttl_secs,
            turn_test_relay_only,
        })
    }

    pub fn metadata_line(&self, now_unix: i64) -> Result<String, TurnCredentialError> {
        turn_metadata_line(
            &self.server,
            self.port,
            &self.secret,
            self.ttl_secs,
            now_unix,
        )
    }
}
