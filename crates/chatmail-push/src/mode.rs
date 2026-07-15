// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Push service mode (`auto` / `on` / `off`) and auto-disable circuit breaker.

use std::sync::atomic::{AtomicU32, Ordering};

use chatmail_db::{get_bool_setting, get_setting, set_setting, settings_keys, DbPool};
use chatmail_types::Result;

/// Consecutive notification-proxy failures before `auto` mode turns push off.
pub const AUTO_DISABLE_AFTER_FAILURES: u32 = 5;

static CONSECUTIVE_FAILURES: AtomicU32 = AtomicU32::new(0);

/// How push is operated: auto, forced on, or forced off (default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushMode {
    /// Enabled; disables after [`AUTO_DISABLE_AFTER_FAILURES`] consecutive failures.
    Auto,
    /// Always enabled (no auto-disable).
    On,
    /// Disabled.
    Off,
}

impl PushMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::On => "on",
            Self::Off => "off",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let v = value.trim().to_ascii_lowercase();
        match v.as_str() {
            "auto" => Some(Self::Auto),
            // Flexible enable: on/true/yes/1/enable/enabled/…
            s if chatmail_config::is_truthy(s) => Some(Self::On),
            // Flexible disable: off/false/no/0/disable/disabled/…
            s if chatmail_config::is_falsy(s) && !s.is_empty() => Some(Self::Off),
            _ => None,
        }
    }

    pub fn runtime_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }
}

pub fn consecutive_failures() -> u32 {
    CONSECUTIVE_FAILURES.load(Ordering::Relaxed)
}

pub fn reset_consecutive_failures() {
    CONSECUTIVE_FAILURES.store(0, Ordering::Relaxed);
}

/// Read configured push mode (default `off`; legacy `__PUSH_ENABLED__` maps to on/off).
pub async fn push_mode(pool: &DbPool) -> Result<PushMode> {
    if let Some(raw) = get_setting(pool, settings_keys::PUSH_MODE).await? {
        if let Some(mode) = PushMode::parse(&raw) {
            return Ok(mode);
        }
    }
    let legacy_on = get_bool_setting(pool, settings_keys::PUSH_ENABLED, false).await?;
    Ok(if legacy_on {
        PushMode::On
    } else {
        PushMode::Off
    })
}

/// Persist mode and keep legacy `__PUSH_ENABLED__` in sync for older admin builds.
pub async fn set_push_mode(pool: &DbPool, mode: PushMode) -> Result<()> {
    set_setting(pool, settings_keys::PUSH_MODE, mode.as_str()).await?;
    set_setting(
        pool,
        settings_keys::PUSH_ENABLED,
        if mode.runtime_enabled() {
            "true"
        } else {
            "false"
        },
    )
    .await?;
    reset_consecutive_failures();
    Ok(())
}

/// Whether push delivery and `XDELTAPUSH` should be active right now.
pub async fn push_runtime_enabled(pool: &DbPool) -> Result<bool> {
    Ok(push_mode(pool).await?.runtime_enabled())
}

/// Successful proxy delivery — resets the consecutive failure counter.
pub fn record_delivery_success() {
    reset_consecutive_failures();
}

/// Failed proxy delivery — in `auto` mode, disables push after repeated failures.
pub async fn record_delivery_failure(pool: &DbPool) {
    let count = CONSECUTIVE_FAILURES.fetch_add(1, Ordering::Relaxed) + 1;
    if count < AUTO_DISABLE_AFTER_FAILURES {
        return;
    }
    let Ok(mode) = push_mode(pool).await else {
        return;
    };
    if mode != PushMode::Auto {
        return;
    }
    if let Err(e) = set_push_mode(pool, PushMode::Off).await {
        tracing::error!(error = %e, "failed to auto-disable push after notification failures");
        return;
    }
    tracing::error!(
        failures = count,
        "push auto-disabled after {AUTO_DISABLE_AFTER_FAILURES} consecutive notification failures; \
         re-enable with `madmail push auto` or `madmail push on`"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::init_memory_db;

    #[tokio::test]
    async fn push_mode_and_circuit_breaker() {
        let pool = init_memory_db().await.unwrap();
        reset_consecutive_failures();

        assert_eq!(push_mode(&pool).await.unwrap(), PushMode::Off);
        assert!(!push_runtime_enabled(&pool).await.unwrap());

        set_push_mode(&pool, PushMode::Auto).await.unwrap();
        record_delivery_failure(&pool).await;
        record_delivery_failure(&pool).await;
        assert_eq!(consecutive_failures(), 2);
        record_delivery_success();
        assert_eq!(consecutive_failures(), 0);

        set_push_mode(&pool, PushMode::Auto).await.unwrap();
        for _ in 0..(AUTO_DISABLE_AFTER_FAILURES - 1) {
            record_delivery_failure(&pool).await;
            assert_eq!(push_mode(&pool).await.unwrap(), PushMode::Auto);
        }
        record_delivery_failure(&pool).await;
        assert_eq!(push_mode(&pool).await.unwrap(), PushMode::Off);
        assert!(!push_runtime_enabled(&pool).await.unwrap());

        let pool_on = init_memory_db().await.unwrap();
        set_push_mode(&pool_on, PushMode::On).await.unwrap();
        reset_consecutive_failures();
        for _ in 0..10 {
            record_delivery_failure(&pool_on).await;
        }
        assert_eq!(push_mode(&pool_on).await.unwrap(), PushMode::On);
    }
}
