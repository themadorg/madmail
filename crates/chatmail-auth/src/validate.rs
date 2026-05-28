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

use chatmail_config::CredentialPolicy;
use chatmail_types::{ChatmailError, Result};

/// Enforce `min_username_length` / `max_username_length` and `password_min_length`
/// (Madmail `chatmail` block + cmrelay `chatmail.ini` parity).
pub fn validate_localpart_and_password(
    policy: &CredentialPolicy,
    username: &str,
    password: &str,
) -> Result<()> {
    let localpart = username
        .split('@')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChatmailError::config("invalid email address"))?;
    let lp_len = localpart.len();
    let min_u = policy.min_username_length as usize;
    let max_u = policy.max_username_length as usize;
    if lp_len < min_u || lp_len > max_u {
        return Err(ChatmailError::config(format!(
            "username localpart must be between {min_u} and {max_u} characters"
        )));
    }
    let min_p = policy.password_min_length as usize;
    if password.len() < min_p {
        return Err(ChatmailError::config(format!(
            "password must be at least {min_p} characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_config::CredentialPolicy;

    #[test]
    fn rejects_short_localpart_and_password() {
        let p = CredentialPolicy::default();
        assert!(validate_localpart_and_password(&p, "ab@x.org", "longenough").is_err());
        assert!(validate_localpart_and_password(&p, "longenough@x.org", "short").is_err());
    }

    #[test]
    fn accepts_valid_lengths() {
        let p = CredentialPolicy::default();
        assert!(validate_localpart_and_password(&p, "12345678@x.org", "12345678").is_ok());
    }

    #[test]
    fn accepts_boundary_max_localpart() {
        let mut p = CredentialPolicy::default();
        p.max_username_length = 12;
        let local = "a".repeat(12);
        assert!(validate_localpart_and_password(&p, &format!("{local}@x.org"), "12345678").is_ok());
    }

    #[test]
    fn rejects_localpart_one_over_max() {
        let mut p = CredentialPolicy::default();
        p.max_username_length = 8;
        let local = "a".repeat(9);
        let err = validate_localpart_and_password(&p, &format!("{local}@x.org"), "12345678")
            .unwrap_err();
        assert!(matches!(err, ChatmailError::Config(msg) if msg.contains("between 8 and 8")));
    }

    #[test]
    fn rejects_invalid_email() {
        let p = CredentialPolicy::default();
        assert!(validate_localpart_and_password(&p, "nouser", "12345678").is_err());
        assert!(validate_localpart_and_password(&p, "@domain.org", "12345678").is_err());
    }

    #[test]
    fn password_error_mentions_min_length() {
        let p = CredentialPolicy::default();
        let err = validate_localpart_and_password(&p, "12345678@x.org", "short")
            .unwrap_err();
        assert!(matches!(err, ChatmailError::Config(msg) if msg.contains("at least 8")));
    }
}
