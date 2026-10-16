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

//! Outbound federation transport allow/block.
//!
//! Missing keys mean allow, which keeps the historical order: HTTPS `/mxdeliv`,
//! then plain HTTP, then SMTP.

use std::sync::RwLock;

use chatmail_db::{get_setting, set_setting, settings_keys, DbPool};
use chatmail_types::{ChatmailError, Result};

/// Permanent failure when https, http, and smtp are all blocked.
pub const ALL_BLOCKED_REASON: &str = "federation transport blocked by policy";

pub const ALLOW: &str = "allow";
pub const BLOCK: &str = "block";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportName {
    Https,
    Http,
    Smtp,
}

impl TransportName {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "https" => Ok(Self::Https),
            "http" => Ok(Self::Http),
            "smtp" => Ok(Self::Smtp),
            _ => Err(ChatmailError::config(
                "transport must be 'https', 'http', or 'smtp'",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Https => "https",
            Self::Http => "http",
            Self::Smtp => "smtp",
        }
    }

    fn settings_key(self) -> &'static str {
        match self {
            Self::Https => settings_keys::FEDERATION_TRANSPORT_HTTPS,
            Self::Http => settings_keys::FEDERATION_TRANSPORT_HTTP,
            Self::Smtp => settings_keys::FEDERATION_TRANSPORT_SMTP,
        }
    }
}

/// `true` means the outbound transport may be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportPolicy {
    pub https: bool,
    pub http: bool,
    pub smtp: bool,
}

impl Default for TransportPolicy {
    fn default() -> Self {
        Self {
            https: true,
            http: true,
            smtp: true,
        }
    }
}

impl TransportPolicy {
    pub fn any_allowed(self) -> bool {
        self.https || self.http || self.smtp
    }

    pub fn label(allowed: bool) -> &'static str {
        if allowed {
            ALLOW
        } else {
            BLOCK
        }
    }

    pub fn mode(self, name: TransportName) -> &'static str {
        let allowed = match name {
            TransportName::Https => self.https,
            TransportName::Http => self.http,
            TransportName::Smtp => self.smtp,
        };
        Self::label(allowed)
    }
}

/// `allow` or `block`. Anything else is an operator error.
pub fn parse_transport_mode(value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "allow" => Ok(true),
        "block" => Ok(false),
        _ => Err(ChatmailError::config(
            "transport mode must be 'allow' or 'block'",
        )),
    }
}

/// Stored `block` disables a transport. Missing or unrecognized values stay allowed.
fn stored_allows(value: Option<String>) -> bool {
    !matches!(
        value.as_deref().map(str::trim),
        Some(v) if v.eq_ignore_ascii_case(BLOCK)
    )
}

pub async fn load_transport_policy(pool: &DbPool) -> Result<TransportPolicy> {
    Ok(TransportPolicy {
        https: stored_allows(get_setting(pool, settings_keys::FEDERATION_TRANSPORT_HTTPS).await?),
        http: stored_allows(get_setting(pool, settings_keys::FEDERATION_TRANSPORT_HTTP).await?),
        smtp: stored_allows(get_setting(pool, settings_keys::FEDERATION_TRANSPORT_SMTP).await?),
    })
}

pub async fn set_transport_mode(pool: &DbPool, name: TransportName, allow: bool) -> Result<()> {
    set_setting(pool, name.settings_key(), TransportPolicy::label(allow)).await
}

#[derive(Debug)]
pub struct FederationTransportPolicy {
    inner: RwLock<TransportPolicy>,
}

impl FederationTransportPolicy {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(TransportPolicy::default()),
        }
    }

    pub fn get(&self) -> TransportPolicy {
        *self.inner.read().expect("transport policy lock")
    }

    pub fn set(&self, policy: TransportPolicy) {
        *self.inner.write().expect("transport policy lock") = policy;
    }

    pub async fn hydrate(&self, pool: &DbPool) -> Result<()> {
        self.set(load_transport_policy(pool).await?);
        Ok(())
    }
}

impl Default for FederationTransportPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::init_memory_db;

    #[tokio::test]
    async fn unset_keys_allow_every_transport() {
        let pool = init_memory_db().await.unwrap();
        let policy = load_transport_policy(&pool).await.unwrap();
        assert_eq!(policy, TransportPolicy::default());
        assert!(policy.any_allowed());
    }

    #[tokio::test]
    async fn block_is_per_transport_and_hydrate_sees_it() {
        let pool = init_memory_db().await.unwrap();
        set_transport_mode(&pool, TransportName::Http, false)
            .await
            .unwrap();
        set_transport_mode(&pool, TransportName::Smtp, false)
            .await
            .unwrap();
        let live = FederationTransportPolicy::new();
        live.hydrate(&pool).await.unwrap();
        let policy = live.get();
        assert!(policy.https);
        assert!(!policy.http);
        assert!(!policy.smtp);
        assert_eq!(policy.mode(TransportName::Http), "block");
    }

    #[tokio::test]
    async fn unrecognized_stored_value_stays_allowed() {
        let pool = init_memory_db().await.unwrap();
        set_setting(&pool, settings_keys::FEDERATION_TRANSPORT_HTTPS, "maybe")
            .await
            .unwrap();
        let policy = load_transport_policy(&pool).await.unwrap();
        assert!(policy.https);
    }

    #[test]
    fn parse_rejects_unknown_mode_and_name() {
        assert!(parse_transport_mode("deny").is_err());
        assert!(TransportName::parse("mx").is_err());
        assert_eq!(TransportName::parse("SMTP").unwrap(), TransportName::Smtp);
        assert!(!parse_transport_mode("block").unwrap());
    }
}
