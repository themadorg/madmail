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

//! TURN REST shared-secret credentials (Madmail / Delta Chat core compatible).

use hmac::{Hmac, Mac};
use sha1::Sha1;
use thiserror::Error;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Error)]
pub enum TurnCredentialError {
    #[error("turn secret must not be empty")]
    EmptySecret,
    #[error("turn server hostname must not be empty")]
    EmptyServer,
}

/// `base64(HMAC-SHA1(secret, username))` — same as Madmail `imap.go` GETMETADATA.
pub fn hmac_turn_password(secret: &str, username: &str) -> Result<String, TurnCredentialError> {
    if secret.is_empty() {
        return Err(TurnCredentialError::EmptySecret);
    }
    let mut mac = HmacSha1::new_from_slice(secret.as_bytes())
        .map_err(|_| TurnCredentialError::EmptySecret)?;
    mac.update(username.as_bytes());
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        mac.finalize().into_bytes(),
    ))
}

/// Build `hostname:port:username:password` for `/shared/vendor/deltachat/turn`.
///
/// `username` is `now_unix + ttl_secs` (expiration timestamp as decimal string).
pub fn turn_metadata_line(
    server: &str,
    port: u16,
    secret: &str,
    ttl_secs: u64,
    now_unix: i64,
) -> Result<String, TurnCredentialError> {
    if server.is_empty() {
        return Err(TurnCredentialError::EmptyServer);
    }
    let username = (now_unix.saturating_add(ttl_secs as i64)).to_string();
    let password = hmac_turn_password(secret, &username)?;
    Ok(format!("{server}:{port}:{username}:{password}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_turn_password_is_deterministic() {
        let once = hmac_turn_password("test-secret", "1700086400").unwrap();
        let again = hmac_turn_password("test-secret", "1700086400").unwrap();
        assert_eq!(once, again);
        assert_ne!(
            once,
            hmac_turn_password("other-secret", "1700086400").unwrap()
        );
    }

    #[test]
    fn turn_metadata_line_format() {
        const NOW: i64 = 1_700_000_000;
        const TTL: u64 = 86_400;
        let expiry = (NOW + TTL as i64).to_string();
        let password = hmac_turn_password("test-secret", &expiry).unwrap();
        let line =
            turn_metadata_line("turn.example.com", 3478, "test-secret", TTL, NOW).unwrap();
        assert_eq!(
            line,
            format!("turn.example.com:3478:{expiry}:{password}")
        );
    }

    #[test]
    fn rejects_empty_secret() {
        assert!(hmac_turn_password("", "1").is_err());
        assert!(turn_metadata_line("h", 3478, "", 60, 1).is_err());
    }
}
