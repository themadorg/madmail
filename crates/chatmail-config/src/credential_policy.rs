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

//! Username/password length limits from the `chatmail { … }` block (Madmail-compatible).

use crate::AppConfig;

/// Length rules for auto-generated credentials and JIT/login validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CredentialPolicy {
    /// `username_length` — random localpart size for `/new` (default 8).
    pub username_length: u32,
    /// `password_length` — random password size for `/new` (default 16).
    pub password_length: u32,
    /// `min_username_length` — minimum localpart length (default 8).
    pub min_username_length: u32,
    /// `max_username_length` — maximum localpart length (default 20).
    pub max_username_length: u32,
    /// `password_min_length` — minimum password length on JIT create / login (default 8).
    pub password_min_length: u32,
}

impl Default for CredentialPolicy {
    fn default() -> Self {
        Self {
            username_length: 8,
            password_length: 16,
            min_username_length: 8,
            max_username_length: 20,
            password_min_length: 8,
        }
    }
}

impl CredentialPolicy {
    /// Random localpart length for registration, clamped to `[min, max]`.
    pub fn generated_username_length(&self) -> usize {
        self.username_length
            .clamp(self.min_username_length, self.max_username_length) as usize
    }

    /// Random password length for registration (at least `password_min_length`).
    pub fn generated_password_length(&self) -> usize {
        self.password_length.max(self.password_min_length) as usize
    }
}

impl AppConfig {
    /// Effective credential policy from `chatmail` directives (Madmail defaults when unset).
    pub fn credential_policy(&self) -> CredentialPolicy {
        let defaults = CredentialPolicy::default();
        let min_u = self
            .min_username_length
            .filter(|&n| n >= 1)
            .unwrap_or(defaults.min_username_length);
        let max_u = self
            .max_username_length
            .unwrap_or(defaults.max_username_length)
            .max(min_u);
        let username_length = self
            .username_length
            .filter(|&n| n >= 1)
            .unwrap_or(defaults.username_length)
            .clamp(min_u, max_u);
        let password_min = self
            .password_min_length
            .filter(|&n| n >= 1)
            .unwrap_or(defaults.password_min_length);
        let password_length = self
            .password_length
            .filter(|&n| n >= 1)
            .unwrap_or(defaults.password_length)
            .max(password_min);
        CredentialPolicy {
            username_length,
            password_length,
            min_username_length: min_u,
            max_username_length: max_u,
            password_min_length: password_min,
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_madmail_chatmail_block() {
        let p = AppConfig::default().credential_policy();
        assert_eq!(p.username_length, 8);
        assert_eq!(p.password_length, 16);
        assert_eq!(p.min_username_length, 8);
        assert_eq!(p.max_username_length, 20);
        assert_eq!(p.password_min_length, 8);
        assert_eq!(p.generated_username_length(), 8);
        assert_eq!(p.generated_password_length(), 16);
    }

    #[test]
    fn generated_username_clamped_to_min_max() {
        let mut cfg = AppConfig::default();
        cfg.username_length = Some(4);
        cfg.min_username_length = Some(8);
        cfg.max_username_length = Some(20);
        assert_eq!(cfg.credential_policy().generated_username_length(), 8);
        cfg.username_length = Some(30);
        assert_eq!(cfg.credential_policy().generated_username_length(), 20);
    }

    #[test]
    fn password_length_at_least_password_min() {
        let mut cfg = AppConfig::default();
        cfg.password_length = Some(6);
        cfg.password_min_length = Some(10);
        let p = cfg.credential_policy();
        assert_eq!(p.password_length, 10);
        assert_eq!(p.generated_password_length(), 10);
    }

    #[test]
    fn max_username_bumped_when_below_min() {
        let mut cfg = AppConfig::default();
        cfg.min_username_length = Some(10);
        cfg.max_username_length = Some(5);
        let p = cfg.credential_policy();
        assert_eq!(p.min_username_length, 10);
        assert_eq!(p.max_username_length, 10);
        assert_eq!(p.generated_username_length(), 10);
    }

    #[test]
    fn madmail_example_min_username_three() {
        let mut cfg = AppConfig::default();
        cfg.min_username_length = Some(3);
        cfg.max_username_length = Some(20);
        cfg.username_length = Some(8);
        let p = cfg.credential_policy();
        assert_eq!(p.min_username_length, 3);
        assert_eq!(p.generated_username_length(), 8);
    }

    #[test]
    fn unset_fields_use_defaults() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.username_length, None);
        let p = cfg.credential_policy();
        assert_eq!(p.min_username_length, 8);
        assert_eq!(p.password_min_length, 8);
    }
}
