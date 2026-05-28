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

use std::io;

use thiserror::Error;

/// Postfix `cleanup_strerror` / cmrelay (`CLEANUP_STAT_SIZE`).
pub const MESSAGE_FILE_TOO_BIG: &str = "552 5.3.4 message file too big";

#[derive(Debug, Error)]
pub enum ChatmailError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("quota exceeded for {user}: used {used} + {incoming} > {max}")]
    QuotaExceeded {
        user: String,
        used: u64,
        incoming: u64,
        max: u64,
    },

    #[error("552 5.3.4 message file too big")]
    MessageTooLarge,

    #[error("storage error: {0}")]
    Storage(String),

    #[error("authentication failed")]
    AuthFailed,

    #[error("user blocked: {0}")]
    UserBlocked(String),

    #[error("encryption needed: {0}")]
    EncryptionNeeded(String),

    #[error("federation rejected: {0}")]
    FederationRejected(String),

    #[error("protocol error: {0}")]
    Protocol(String),
}

impl ChatmailError {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub fn storage(msg: impl Into<String>) -> Self {
        Self::Storage(msg.into())
    }

    pub fn protocol(msg: impl Into<String>) -> Self {
        Self::Protocol(msg.into())
    }

    pub fn message_too_large() -> Self {
        Self::MessageTooLarge
    }

    pub fn is_message_too_large(&self) -> bool {
        matches!(self, Self::MessageTooLarge)
    }
}

pub type Result<T> = std::result::Result<T, ChatmailError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_file_too_big_constant_matches_display() {
        assert_eq!(MESSAGE_FILE_TOO_BIG, "552 5.3.4 message file too big");
        let err = ChatmailError::message_too_large();
        assert!(err.is_message_too_large());
        assert_eq!(format!("{err}"), "552 5.3.4 message file too big");
    }

    #[test]
    fn constructor_helpers_build_expected_variants() {
        assert!(matches!(ChatmailError::config("bad"), ChatmailError::Config(_)));
        assert!(matches!(
            ChatmailError::storage("missing"),
            ChatmailError::Storage(_)
        ));
        assert!(matches!(
            ChatmailError::protocol("syntax"),
            ChatmailError::Protocol(_)
        ));
    }

    #[test]
    fn quota_exceeded_formats_user_and_limits() {
        let err = ChatmailError::QuotaExceeded {
            user: "alice@test".into(),
            used: 100,
            incoming: 50,
            max: 120,
        };
        let msg = format!("{err}");
        assert!(msg.contains("alice@test"));
        assert!(msg.contains("quota exceeded"));
        assert!(msg.contains("100"));
        assert!(msg.contains("120"));
    }

    #[test]
    fn io_error_converts_via_from() {
        let err: ChatmailError =
            std::io::Error::new(std::io::ErrorKind::NotFound, "gone").into();
        assert!(matches!(err, ChatmailError::Io(_)));
        assert!(format!("{err}").contains("gone"));
    }

    #[test]
    fn policy_errors_have_stable_prefixes() {
        assert!(format!("{}", ChatmailError::AuthFailed).contains("authentication"));
        assert!(format!("{}", ChatmailError::UserBlocked("x".into())).contains("blocked"));
        assert!(format!(
            "{}",
            ChatmailError::EncryptionNeeded("pgp".into())
        )
        .contains("encryption"));
        assert!(format!(
            "{}",
            ChatmailError::FederationRejected("evil".into())
        )
        .contains("federation"));
    }
}
