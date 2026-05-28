// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Message file retention settings (`__MESSAGE_RETENTION_*__` in `settings`).

use std::time::Duration;

use chatmail_config::parse_duration;
use chatmail_types::Result;

use crate::{get_bool_setting, get_setting, settings_keys, DbPool};

/// Stored retention duration (e.g. `30d`, `720h`).
pub fn format_retention_days(days: u32) -> String {
    format!("{days}d")
}

/// Whole days for admin UI; `None` if unset or unparseable.
pub fn retention_days_from_value(value: &str) -> Option<u32> {
    let d = parse_duration(value.trim()).ok()?;
    let days = d.as_secs() / 86_400;
    if days == 0 {
        return None;
    }
    Some(days as u32)
}

pub fn duration_from_value(value: &str) -> Option<Duration> {
    parse_duration(value.trim()).ok()
}

pub const DEFAULT_RETENTION_DAYS: u32 = 30;

/// Snapshot for `GET /admin/status` (`message_retention` field).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRetentionStatus {
    pub enabled: bool,
    pub days: u32,
    pub retention: String,
}

/// Whether automatic maildir file rotation (`prune-old-messages`) should run.
pub async fn message_retention_enabled(pool: &DbPool) -> Result<bool> {
    get_bool_setting(pool, settings_keys::MESSAGE_RETENTION_ENABLED, false).await
}

/// Current rotation settings for the admin dashboard (`GET /admin/status`).
pub async fn message_retention_status(pool: &DbPool) -> Result<MessageRetentionStatus> {
    let enabled = message_retention_enabled(pool).await?;
    let retention = match get_setting(pool, settings_keys::MESSAGE_RETENTION).await? {
        Some(raw) if duration_from_value(&raw).is_some() => raw,
        _ => format_retention_days(DEFAULT_RETENTION_DAYS),
    };
    let days = retention_days_from_value(&retention).unwrap_or(DEFAULT_RETENTION_DAYS);
    Ok(MessageRetentionStatus {
        enabled,
        days,
        retention,
    })
}

/// Effective retention for the hourly purge job (DB only).
pub async fn effective_message_retention(pool: &DbPool) -> Result<Option<Duration>> {
    if !message_retention_enabled(pool).await? {
        return Ok(None);
    }
    let Some(raw) = get_setting(pool, settings_keys::MESSAGE_RETENTION).await? else {
        return Ok(None);
    };
    Ok(duration_from_value(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{init_memory_db, set_setting};

    #[test]
    fn days_round_trip() {
        assert_eq!(retention_days_from_value("30d"), Some(30));
        assert_eq!(format_retention_days(7), "7d");
    }

    #[tokio::test]
    async fn effective_retention_requires_db_toggle() {
        let pool = init_memory_db().await.unwrap();
        assert!(effective_message_retention(&pool).await.unwrap().is_none());
        set_setting(&pool, settings_keys::MESSAGE_RETENTION_ENABLED, "true")
            .await
            .unwrap();
        set_setting(&pool, settings_keys::MESSAGE_RETENTION, "7d")
            .await
            .unwrap();
        let d = effective_message_retention(&pool).await.unwrap().unwrap();
        assert_eq!(d.as_secs(), 7 * 86_400);
    }
}
