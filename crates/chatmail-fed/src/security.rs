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

//! Federation recipient domain checks (Madmail `mxdeliv_security.go`).

pub use chatmail_db::{is_federation_rcpt_blocked, is_federation_sender_blocked};

/// Recipient domain belongs to this server (all normalized forms).
pub fn recipient_matches_server(rcpt: &str, accepted_domains: &[String]) -> bool {
    chatmail_types::address_is_local(rcpt, accepted_domains)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_admin_local_part() {
        assert!(is_federation_rcpt_blocked("admin@example.org"));
        assert!(!is_federation_rcpt_blocked("alice@example.org"));
    }

    /// P7-UT01: recipient validation blocks reserved local parts.
    #[test]
    fn p7_ut01_test_recipient_not_admin() {
        assert!(is_federation_rcpt_blocked("postmaster@example.org"));
        let local = chatmail_types::build_local_domains("example.org", None);
        assert!(recipient_matches_server("user@example.org", &local));
        assert!(!recipient_matches_server("user@other.org", &local));
    }
}
