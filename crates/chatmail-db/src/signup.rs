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

//! Public sign-up switches and the rule that combines them.
//!
//! The auth cache, the `/new` handler, the registration page, the admin API
//! and the CLI all read these settings. Keep the defaults and the gate here so
//! they cannot drift apart (#235).

use std::collections::HashMap;

use chatmail_types::Result;

use crate::settings::get_bool_setting;
use crate::settings_keys;
use crate::DbPool;

/// `__REGISTRATION_OPEN__` when the row is missing. Matches [`crate::seed_install_defaults`].
pub const REGISTRATION_OPEN_DEFAULT: bool = true;
/// `__JIT_REGISTRATION_ENABLED__` when the row is missing. Matches [`crate::seed_install_defaults`].
pub const JIT_REGISTRATION_ENABLED_DEFAULT: bool = true;
/// `__REGISTRATION_TOKEN_REQUIRED__` when the row is missing.
pub const REGISTRATION_TOKEN_REQUIRED_DEFAULT: bool = false;

/// Settings keys read by [`SignupSettings`].
pub const SIGNUP_SETTINGS_KEYS: &[&str] = &[
    settings_keys::REGISTRATION_OPEN,
    settings_keys::JIT_REGISTRATION_ENABLED,
    settings_keys::REGISTRATION_TOKEN_REQUIRED,
];

/// The stored sign-up switches, with missing rows replaced by their defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignupSettings {
    pub registration_open: bool,
    pub jit_enabled: bool,
    pub token_required: bool,
}

impl Default for SignupSettings {
    fn default() -> Self {
        Self {
            registration_open: REGISTRATION_OPEN_DEFAULT,
            jit_enabled: JIT_REGISTRATION_ENABLED_DEFAULT,
            token_required: REGISTRATION_TOKEN_REQUIRED_DEFAULT,
        }
    }
}

impl SignupSettings {
    /// Read each switch with [`get_bool_setting`], the same lookup order the
    /// auth cache has always used (Madmail key/value rows first on legacy
    /// layouts).
    pub async fn load(pool: &DbPool) -> Result<Self> {
        Ok(Self {
            registration_open: get_bool_setting(
                pool,
                settings_keys::REGISTRATION_OPEN,
                REGISTRATION_OPEN_DEFAULT,
            )
            .await?,
            jit_enabled: get_bool_setting(
                pool,
                settings_keys::JIT_REGISTRATION_ENABLED,
                JIT_REGISTRATION_ENABLED_DEFAULT,
            )
            .await?,
            token_required: get_bool_setting(
                pool,
                settings_keys::REGISTRATION_TOKEN_REQUIRED,
                REGISTRATION_TOKEN_REQUIRED_DEFAULT,
            )
            .await?,
        })
    }

    /// Build from a batch loaded with [`crate::get_settings_many`] (the www
    /// settings snapshot). Keys that are absent from `map` take their defaults.
    pub fn from_map(map: &HashMap<String, String>) -> Self {
        let get = |key: &str, default: bool| {
            map.get(key)
                .map(|v| chatmail_config::parse_bool_str(v))
                .unwrap_or(default)
        };
        Self {
            registration_open: get(settings_keys::REGISTRATION_OPEN, REGISTRATION_OPEN_DEFAULT),
            jit_enabled: get(
                settings_keys::JIT_REGISTRATION_ENABLED,
                JIT_REGISTRATION_ENABLED_DEFAULT,
            ),
            token_required: get(
                settings_keys::REGISTRATION_TOKEN_REQUIRED,
                REGISTRATION_TOKEN_REQUIRED_DEFAULT,
            ),
        }
    }

    /// Whether a first IMAP or SMTP login with an unknown address may create
    /// the account.
    ///
    /// Needs registration open and JIT enabled. IMAP and SMTP cannot carry an
    /// invite token, so a required token turns this path off as well; invited
    /// users sign up through `POST /new`.
    pub fn password_signup_allowed(&self) -> bool {
        self.registration_open && self.jit_enabled && !self.token_required
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{init_memory_db, seed_install_defaults, set_setting};

    #[tokio::test]
    async fn missing_rows_use_defaults() {
        let pool = init_memory_db().await.unwrap();
        let s = SignupSettings::load(&pool).await.unwrap();
        assert_eq!(s, SignupSettings::default());
        assert!(s.registration_open);
        assert!(s.jit_enabled);
        assert!(!s.token_required);
        assert!(s.password_signup_allowed());
    }

    #[tokio::test]
    async fn defaults_match_install_seed() {
        let pool = init_memory_db().await.unwrap();
        seed_install_defaults(&pool).await.unwrap();
        assert_eq!(
            SignupSettings::load(&pool).await.unwrap(),
            SignupSettings::default()
        );
    }

    #[tokio::test]
    async fn gate_needs_open_jit_and_no_token() {
        let pool = init_memory_db().await.unwrap();
        for open in [false, true] {
            for jit in [false, true] {
                for token in [false, true] {
                    let b = |v: bool| if v { "true" } else { "false" };
                    set_setting(&pool, settings_keys::REGISTRATION_OPEN, b(open))
                        .await
                        .unwrap();
                    set_setting(&pool, settings_keys::JIT_REGISTRATION_ENABLED, b(jit))
                        .await
                        .unwrap();
                    set_setting(&pool, settings_keys::REGISTRATION_TOKEN_REQUIRED, b(token))
                        .await
                        .unwrap();
                    let s = SignupSettings::load(&pool).await.unwrap();
                    let batch = crate::get_settings_many(&pool, SIGNUP_SETTINGS_KEYS)
                        .await
                        .unwrap();
                    assert_eq!(SignupSettings::from_map(&batch), s);
                    assert_eq!(
                        s,
                        SignupSettings {
                            registration_open: open,
                            jit_enabled: jit,
                            token_required: token,
                        }
                    );
                    assert_eq!(
                        s.password_signup_allowed(),
                        open && jit && !token,
                        "open={open} jit={jit} token={token}"
                    );
                }
            }
        }
    }
}
