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

//! Human-readable policy lines for www templates (info page).

use std::time::Duration;

use chatmail_config::{parse_duration, AppConfig};

/// Label for `storage.imapsql retention` (e.g. `24h` → `24 hours`, `720h` → `30 days`).
pub fn format_retention_label(config: &AppConfig) -> Option<String> {
    let raw = config.retention.as_deref()?.trim();
    if raw.is_empty() || raw == "0" {
        return None;
    }
    let d = parse_duration(raw).ok()?;
    if d.is_zero() {
        return None;
    }
    Some(humanize_duration(d))
}

/// Full retention bullet for the info page, localized.
pub fn retention_info_line(language: &str, label: &str) -> String {
    match language {
        "fa" => format!(
            "پیام‌ها پس از {label} به طور خودکار از سرور پاک می‌شوند."
        ),
        "ru" => format!("Сообщения автоматически удаляются с сервера через {label}."),
        "es" => format!(
            "Los mensajes se eliminan automáticamente del servidor después de {label}."
        ),
        _ => format!("Messages are automatically deleted from the server after {label}."),
    }
}

fn humanize_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 86400 && secs % 86400 == 0 {
        let days = secs / 86400;
        return if days == 1 {
            "1 day".into()
        } else {
            format!("{days} days")
        };
    }
    if secs >= 3600 && secs % 3600 == 0 {
        let hours = secs / 3600;
        return if hours == 1 {
            "1 hour".into()
        } else {
            format!("{hours} hours")
        };
    }
    if secs >= 60 && secs % 60 == 0 {
        let minutes = secs / 60;
        return if minutes == 1 {
            "1 minute".into()
        } else {
            format!("{minutes} minutes")
        };
    }
    if secs == 1 {
        "1 second".into()
    } else {
        format!("{secs} seconds")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_config::AppConfig;

    #[test]
    fn retention_label_from_config() {
        let mut cfg = AppConfig::default();
        cfg.retention = Some("720h".into());
        assert_eq!(format_retention_label(&cfg).as_deref(), Some("30 days"));
        cfg.retention = Some("24h".into());
        assert_eq!(format_retention_label(&cfg).as_deref(), Some("1 day"));
        cfg.retention = Some("36h".into());
        assert_eq!(format_retention_label(&cfg).as_deref(), Some("36 hours"));
        cfg.retention = None;
        assert!(format_retention_label(&cfg).is_none());
    }
}
