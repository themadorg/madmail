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

//! Shared HTTP client for federation (`/mxdeliv`) delivery.
//!
//! `reqwest::Client` must be reused across requests; building one per message wastes
//! memory (connection pools, TLS) and prevents connection reuse.

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;

static FEDERATION_HTTP: OnceLock<Client> = OnceLock::new();

/// Process-wide federation HTTP client (initialized on first use).
pub fn federation_http_client() -> &'static Client {
    FEDERATION_HTTP.get_or_init(|| {
        Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(16)
            .build()
            .expect("federation reqwest client")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_is_singleton() {
        let a = federation_http_client() as *const Client;
        let b = federation_http_client() as *const Client;
        assert_eq!(a, b);
    }
}
