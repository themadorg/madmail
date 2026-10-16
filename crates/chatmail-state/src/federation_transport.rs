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
//! Missing global keys mean allow, which keeps the historical order: HTTPS
//! `/mxdeliv`, then plain HTTP, then SMTP. A destination domain may override
//! any of those three. A missing override inherits the global mode.

use std::collections::BTreeMap;
use std::sync::RwLock;

use chatmail_db::{
    delete_setting, get_setting, normalize_federation_domain, set_setting, settings_keys, DbPool,
};
use chatmail_types::{ChatmailError, Result};
use serde_json::{Map, Value};

/// Permanent failure when https, http, and smtp are all blocked.
pub const ALL_BLOCKED_REASON: &str = "federation transport blocked by policy";

pub const ALLOW: &str = "allow";
pub const BLOCK: &str = "block";
pub const INHERIT: &str = "inherit";

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

    fn with_override(self, name: TransportName, mode: Option<bool>) -> Self {
        let mut next = self;
        if let Some(allowed) = mode {
            match name {
                TransportName::Https => next.https = allowed,
                TransportName::Http => next.http = allowed,
                TransportName::Smtp => next.smtp = allowed,
            }
        }
        next
    }
}

/// Explicit per-domain modes. `None` inherits the global setting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DomainTransportOverride {
    pub https: Option<bool>,
    pub http: Option<bool>,
    pub smtp: Option<bool>,
}

impl DomainTransportOverride {
    pub fn is_empty(&self) -> bool {
        self.https.is_none() && self.http.is_none() && self.smtp.is_none()
    }

    pub fn get(&self, name: TransportName) -> Option<bool> {
        match name {
            TransportName::Https => self.https,
            TransportName::Http => self.http,
            TransportName::Smtp => self.smtp,
        }
    }

    fn set(&mut self, name: TransportName, mode: Option<bool>) {
        match name {
            TransportName::Https => self.https = mode,
            TransportName::Http => self.http = mode,
            TransportName::Smtp => self.smtp = mode,
        }
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

/// `allow`, `block`, or `inherit` (`None`).
pub fn parse_override_mode(value: &str) -> Result<Option<bool>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "allow" => Ok(Some(true)),
        "block" => Ok(Some(false)),
        "inherit" => Ok(None),
        _ => Err(ChatmailError::config(
            "transport mode must be 'allow', 'block', or 'inherit'",
        )),
    }
}

/// Destination domain for a transport override. Brackets are stripped and the
/// name is lowercased, matching federation policy domains.
pub fn normalize_transport_domain(domain: &str) -> Result<String> {
    let domain = normalize_federation_domain(domain);
    let invalid = domain.is_empty()
        || domain
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '@' | '/' | '\\' | '?' | '#'));
    if invalid {
        return Err(ChatmailError::config("domain must be a destination domain"));
    }
    Ok(domain)
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

pub async fn load_domain_overrides(
    pool: &DbPool,
) -> Result<BTreeMap<String, DomainTransportOverride>> {
    let raw = get_setting(pool, settings_keys::FEDERATION_TRANSPORT_DOMAINS).await?;
    Ok(parse_domain_overrides(raw.as_deref().unwrap_or("")))
}

/// Apply override updates for one destination domain and persist the map.
/// `None` removes that transport's override. An empty map deletes the setting.
pub async fn update_domain_transports(
    pool: &DbPool,
    domain: &str,
    updates: &[(TransportName, Option<bool>)],
) -> Result<String> {
    let domain = normalize_transport_domain(domain)?;
    let mut map = load_domain_overrides(pool).await?;
    let mut entry = map.remove(&domain).unwrap_or_default();
    for (name, mode) in updates {
        entry.set(*name, *mode);
    }
    if !entry.is_empty() {
        map.insert(domain.clone(), entry);
    }
    save_domain_overrides(pool, &map).await?;
    Ok(domain)
}

pub fn domain_overrides_json(map: &BTreeMap<String, DomainTransportOverride>) -> Vec<Value> {
    map.iter()
        .filter(|(_, ov)| !ov.is_empty())
        .map(|(domain, ov)| {
            let mut obj = Map::new();
            obj.insert("domain".into(), Value::String(domain.clone()));
            for name in [
                TransportName::Https,
                TransportName::Http,
                TransportName::Smtp,
            ] {
                if let Some(allowed) = ov.get(name) {
                    obj.insert(
                        name.as_str().into(),
                        Value::String(TransportPolicy::label(allowed).into()),
                    );
                }
            }
            Value::Object(obj)
        })
        .collect()
}

fn parse_domain_overrides(raw: &str) -> BTreeMap<String, DomainTransportOverride> {
    let raw = raw.trim();
    if raw.is_empty() {
        return BTreeMap::new();
    }
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        tracing::warn!("ignoring corrupt federation transport domain overrides");
        return BTreeMap::new();
    };
    let Some(obj) = value.as_object() else {
        tracing::warn!("ignoring federation transport domain overrides that are not an object");
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    for (domain, body) in obj {
        let Ok(domain) = normalize_transport_domain(domain) else {
            continue;
        };
        let Some(body) = body.as_object() else {
            continue;
        };
        let mut ov = DomainTransportOverride::default();
        apply_stored_mode(&mut ov.https, body.get("https"));
        apply_stored_mode(&mut ov.http, body.get("http"));
        apply_stored_mode(&mut ov.smtp, body.get("smtp"));
        if ov.is_empty() {
            continue;
        }
        out.entry(domain)
            .and_modify(|existing| merge_override(existing, &ov))
            .or_insert(ov);
    }
    out
}

fn apply_stored_mode(slot: &mut Option<bool>, value: Option<&Value>) {
    let Some(text) = value.and_then(Value::as_str) else {
        return;
    };
    if text.trim().eq_ignore_ascii_case(ALLOW) {
        *slot = Some(true);
    } else if text.trim().eq_ignore_ascii_case(BLOCK) {
        *slot = Some(false);
    }
}

fn merge_override(existing: &mut DomainTransportOverride, incoming: &DomainTransportOverride) {
    if incoming.https.is_some() {
        existing.https = incoming.https;
    }
    if incoming.http.is_some() {
        existing.http = incoming.http;
    }
    if incoming.smtp.is_some() {
        existing.smtp = incoming.smtp;
    }
}

async fn save_domain_overrides(
    pool: &DbPool,
    map: &BTreeMap<String, DomainTransportOverride>,
) -> Result<()> {
    let mut obj = Map::new();
    for (domain, ov) in map {
        if ov.is_empty() {
            continue;
        }
        let mut body = Map::new();
        for name in [
            TransportName::Https,
            TransportName::Http,
            TransportName::Smtp,
        ] {
            if let Some(allowed) = ov.get(name) {
                body.insert(
                    name.as_str().into(),
                    Value::String(TransportPolicy::label(allowed).into()),
                );
            }
        }
        if !body.is_empty() {
            obj.insert(domain.clone(), Value::Object(body));
        }
    }
    if obj.is_empty() {
        return delete_setting(pool, settings_keys::FEDERATION_TRANSPORT_DOMAINS).await;
    }
    let text = serde_json::to_string(&Value::Object(obj)).map_err(|e| {
        ChatmailError::config(format!("federation transport domain overrides: {e}"))
    })?;
    set_setting(pool, settings_keys::FEDERATION_TRANSPORT_DOMAINS, &text).await
}

#[derive(Debug, Clone)]
struct CachedTransport {
    global: TransportPolicy,
    overrides: BTreeMap<String, DomainTransportOverride>,
}

#[derive(Debug)]
pub struct FederationTransportPolicy {
    inner: RwLock<CachedTransport>,
}

impl FederationTransportPolicy {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(CachedTransport {
                global: TransportPolicy::default(),
                overrides: BTreeMap::new(),
            }),
        }
    }

    pub fn get(&self) -> TransportPolicy {
        self.inner.read().expect("transport policy lock").global
    }

    /// Replace the global allow/block. Domain overrides stay in place.
    pub fn set(&self, policy: TransportPolicy) {
        self.inner.write().expect("transport policy lock").global = policy;
    }

    pub fn overrides(&self) -> BTreeMap<String, DomainTransportOverride> {
        self.inner
            .read()
            .expect("transport policy lock")
            .overrides
            .clone()
    }

    /// Global policy with this destination domain's overrides applied.
    pub fn effective(&self, domain: &str) -> TransportPolicy {
        let cached = self.inner.read().expect("transport policy lock");
        let key = normalize_federation_domain(domain);
        let Some(ov) = cached.overrides.get(&key) else {
            return cached.global;
        };
        let mut policy = cached.global;
        for name in [
            TransportName::Https,
            TransportName::Http,
            TransportName::Smtp,
        ] {
            policy = policy.with_override(name, ov.get(name));
        }
        policy
    }

    pub async fn hydrate(&self, pool: &DbPool) -> Result<()> {
        let global = load_transport_policy(pool).await?;
        let overrides = load_domain_overrides(pool).await?;
        let mut guard = self.inner.write().expect("transport policy lock");
        guard.global = global;
        guard.overrides = overrides;
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
        assert!(load_domain_overrides(&pool).await.unwrap().is_empty());
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
        assert!(parse_override_mode("deny").is_err());
        assert!(TransportName::parse("mx").is_err());
        assert_eq!(TransportName::parse("SMTP").unwrap(), TransportName::Smtp);
        assert!(!parse_transport_mode("block").unwrap());
        assert_eq!(parse_override_mode("inherit").unwrap(), None);
    }

    #[tokio::test]
    async fn domain_override_inherits_unset_transports() {
        let pool = init_memory_db().await.unwrap();
        set_transport_mode(&pool, TransportName::Smtp, false)
            .await
            .unwrap();
        update_domain_transports(
            &pool,
            "Evil.TEST",
            &[
                (TransportName::Http, Some(false)),
                (TransportName::Https, Some(false)),
            ],
        )
        .await
        .unwrap();
        let stored = get_setting(&pool, settings_keys::FEDERATION_TRANSPORT_DOMAINS)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored, r#"{"evil.test":{"http":"block","https":"block"}}"#);

        let live = FederationTransportPolicy::new();
        live.hydrate(&pool).await.unwrap();
        let effective = live.effective("[Evil.TEST]");
        assert!(!effective.https);
        assert!(!effective.http);
        assert!(!effective.smtp);
        assert!(live.effective("other.org").https);
        assert!(live.get().https);

        live.set(TransportPolicy::default());
        assert!(live.get().smtp);
        assert!(!live.effective("evil.test").http);

        update_domain_transports(&pool, "evil.test", &[(TransportName::Http, None)])
            .await
            .unwrap();
        live.hydrate(&pool).await.unwrap();
        assert!(!live.effective("evil.test").https);
        assert!(live.effective("evil.test").http);

        update_domain_transports(&pool, "evil.test", &[(TransportName::Https, None)])
            .await
            .unwrap();
        assert!(
            get_setting(&pool, settings_keys::FEDERATION_TRANSPORT_DOMAINS)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn corrupt_domain_json_is_ignored() {
        let pool = init_memory_db().await.unwrap();
        set_setting(
            &pool,
            settings_keys::FEDERATION_TRANSPORT_DOMAINS,
            "not-json",
        )
        .await
        .unwrap();
        let live = FederationTransportPolicy::new();
        live.hydrate(&pool).await.unwrap();
        assert!(live.overrides().is_empty());
        assert_eq!(live.effective("evil.test"), TransportPolicy::default());

        set_setting(
            &pool,
            settings_keys::FEDERATION_TRANSPORT_DOMAINS,
            r#"{"evil.test":{"http":"sometimes","smtp":"block"}}"#,
        )
        .await
        .unwrap();
        live.hydrate(&pool).await.unwrap();
        let ov = live.overrides().remove("evil.test").unwrap();
        assert!(ov.http.is_none());
        assert_eq!(ov.smtp, Some(false));
    }

    #[test]
    fn normalize_rejects_empty_and_addresses() {
        assert!(normalize_transport_domain("  ").is_err());
        assert!(normalize_transport_domain("a@b.test").is_err());
        assert_eq!(normalize_transport_domain("[1.1.1.1]").unwrap(), "1.1.1.1");
    }
}
