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

use chatmail_types::{wrap_ip_domain, ChatmailError, Result};

/// Normalize email username (Madmail `auth.NormalizeUsername`: case-fold + IP brackets).
pub fn normalize_username(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.contains('@') {
        return Err(ChatmailError::config("invalid email address"));
    }
    let (local, domain) = trimmed
        .rsplit_once('@')
        .ok_or_else(|| ChatmailError::config("invalid email address"))?;
    Ok(format!(
        "{}@{}",
        local.to_ascii_lowercase(),
        wrap_ip_domain(domain).to_ascii_lowercase()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P3-UT01
    #[test]
    fn p3_ut01_test_precis_casefold() {
        assert_eq!(
            normalize_username("User@Domain.com").unwrap(),
            "user@domain.com"
        );
        assert_eq!(
            normalize_username("User@1.2.3.4").unwrap(),
            "user@[1.2.3.4]"
        );
    }
}
