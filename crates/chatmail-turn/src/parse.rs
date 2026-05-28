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

//! Parse TURN metadata lines (Delta Chat core compatible).

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTurnMetadata {
    pub hostname: String,
    pub port: u16,
    pub expiration_timestamp: i64,
    pub password: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseTurnMetadataError {
    #[error("missing hostname")]
    MissingHostname,
    #[error("missing port")]
    MissingPort,
    #[error("invalid port")]
    InvalidPort,
    #[error("missing expiration timestamp")]
    MissingTimestamp,
    #[error("invalid expiration timestamp")]
    InvalidTimestamp,
    #[error("missing password")]
    MissingPassword,
}

/// Parse `hostname:port:username:password` from IMAP METADATA.
pub fn parse_turn_metadata(metadata: &str) -> Result<ParsedTurnMetadata, ParseTurnMetadataError> {
    let (hostname, rest) = metadata
        .split_once(':')
        .ok_or(ParseTurnMetadataError::MissingHostname)?;
    let (port, rest) = rest.split_once(':').ok_or(ParseTurnMetadataError::MissingPort)?;
    let port = port
        .parse()
        .map_err(|_| ParseTurnMetadataError::InvalidPort)?;
    let (ts, password) = rest
        .split_once(':')
        .ok_or(ParseTurnMetadataError::MissingTimestamp)?;
    let expiration_timestamp = ts
        .parse()
        .map_err(|_| ParseTurnMetadataError::InvalidTimestamp)?;
    if password.is_empty() {
        return Err(ParseTurnMetadataError::MissingPassword);
    }
    Ok(ParsedTurnMetadata {
        hostname: hostname.to_string(),
        port,
        expiration_timestamp,
        password: password.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn_metadata_line;

    #[test]
    fn p9_ut02_parses_metadata_line() {
        let line =
            turn_metadata_line("example.com", 3478, "test-secret", 86400, 1758564468).unwrap();
        let parsed = parse_turn_metadata(&line).unwrap();
        assert_eq!(parsed.hostname, "example.com");
        assert_eq!(parsed.port, 3478);
        assert_eq!(parsed.expiration_timestamp, 1758650868);
        assert_eq!(parsed.password, "nkfvRIGUcx0N/jq/StNLPajpLZE=");
    }

    #[test]
    fn p9_ut02_roundtrip_with_credentials() {
        let line =
            crate::turn_metadata_line("127.0.0.1", 3478, "s3cret", 3600, 1_700_000_000).unwrap();
        let parsed = parse_turn_metadata(&line).unwrap();
        assert_eq!(parsed.hostname, "127.0.0.1");
        assert_eq!(parsed.port, 3478);
        assert_eq!(parsed.expiration_timestamp, 1_700_003_600);
    }
}
