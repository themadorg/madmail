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

//! Parse Madmail/maddy `DataSize` values (`1G`, `10M`, `32K`, …).

use chatmail_types::ChatmailError;

/// Madmail `storage.imapsql` default when `default_quota` is unset (1 GiB).
pub const DEFAULT_QUOTA_BYTES: u64 = 1024 * 1024 * 1024;

/// Default cap when config and DB omit `appendlimit` / `max_message_size` (100 MiB).
pub const DEFAULT_MAX_MESSAGE_BYTES: u64 = 100 * 1024 * 1024;

/// Default human-readable size for install / DB seed.
pub const DEFAULT_MAX_MESSAGE_SIZE: &str = "100M";

/// Effective server-wide default quota: `default_quota` from config, else [`DEFAULT_QUOTA_BYTES`].
pub fn effective_default_quota_bytes(config: &crate::AppConfig) -> u64 {
    config
        .default_quota
        .as_deref()
        .and_then(|s| parse_data_size(s).ok())
        .unwrap_or(DEFAULT_QUOTA_BYTES)
}

/// Effective cap from `maddy.conf` only (`appendlimit` ∧ `max_message_size`).
pub fn effective_max_message_bytes(config: &crate::AppConfig) -> u64 {
    let append = config
        .appendlimit
        .as_deref()
        .and_then(|s| parse_data_size(s).ok());
    let smtp = config
        .max_message_size
        .as_deref()
        .and_then(|s| parse_data_size(s).ok());
    match (append, smtp) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => DEFAULT_MAX_MESSAGE_BYTES,
    }
}

/// Effective cap from config file + optional DB overrides (`__APPENDLIMIT__`, `__MAX_MESSAGE_SIZE__`).
pub fn resolve_max_message_bytes(
    config_effective: u64,
    append_db: Option<&str>,
    max_db: Option<&str>,
) -> Result<u64, ChatmailError> {
    let append = append_db.map(parse_data_size).transpose()?;
    let max = max_db.map(parse_data_size).transpose()?;
    Ok(match (append, max) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => config_effective,
    })
}

/// Format bytes as a Madmail-style size token (e.g. `100M`, `1G`).
pub fn format_data_size(bytes: u64) -> String {
    const G: u64 = 1024 * 1024 * 1024;
    const M: u64 = 1024 * 1024;
    const K: u64 = 1024;
    if bytes >= G && bytes % G == 0 {
        format!("{}G", bytes / G)
    } else if bytes >= M && bytes % M == 0 {
        format!("{}M", bytes / M)
    } else if bytes >= K && bytes % K == 0 {
        format!("{}K", bytes / K)
    } else {
        format!("{bytes}B")
    }
}

/// Parse a single size token (e.g. `1G`, `10M`). Multiple tokens are not summed here;
/// Madmail config uses one value per directive in practice.
pub fn parse_data_size(s: &str) -> Result<u64, ChatmailError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(ChatmailError::config("missing a number"));
    }

    let mut total: u64 = 0;
    let mut current_digit = String::new();
    let mut suffix = String::new();

    for ch in s.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() {
            if !suffix.is_empty() {
                return Err(ChatmailError::config("unexpected digit after a suffix"));
            }
            current_digit.push(ch);
            continue;
        }
        if ch != ' ' {
            suffix.push(ch);
            continue;
        }

        if current_digit.is_empty() && suffix.is_empty() {
            continue;
        }

        let num: u64 = current_digit
            .parse()
            .map_err(|e| ChatmailError::config(format!("invalid data size: {e}")))?;

        match suffix.as_str() {
            "G" => total = total.saturating_add(num.saturating_mul(1024 * 1024 * 1024)),
            "M" => total = total.saturating_add(num.saturating_mul(1024 * 1024)),
            "K" => total = total.saturating_add(num.saturating_mul(1024)),
            "B" | "b" => total = total.saturating_add(num),
            "" if num == 0 => {}
            other => {
                return Err(ChatmailError::config(format!(
                    "unknown unit suffix: {other}"
                )));
            }
        }

        current_digit.clear();
        suffix.clear();
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_1g() {
        assert_eq!(parse_data_size("1G").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn parse_10m() {
        assert_eq!(parse_data_size("10M").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn effective_from_config() {
        let cfg = crate::AppConfig {
            default_quota: Some("1G".into()),
            ..Default::default()
        };
        assert_eq!(effective_default_quota_bytes(&cfg), 1024 * 1024 * 1024);
    }

    #[test]
    fn effective_without_config_uses_madmail_default() {
        assert_eq!(
            effective_default_quota_bytes(&crate::AppConfig::default()),
            DEFAULT_QUOTA_BYTES
        );
    }

    #[test]
    fn effective_max_message_bytes_defaults_to_100m() {
        assert_eq!(
            effective_max_message_bytes(&crate::AppConfig::default()),
            100 * 1024 * 1024
        );
    }

    #[test]
    fn resolve_max_message_bytes_db_overrides_config() {
        let config_eff = 100 * 1024 * 1024;
        assert_eq!(
            resolve_max_message_bytes(config_eff, Some("200M"), None).unwrap(),
            200 * 1024 * 1024
        );
        assert_eq!(
            resolve_max_message_bytes(config_eff, Some("80M"), Some("40M")).unwrap(),
            40 * 1024 * 1024
        );
        assert_eq!(
            resolve_max_message_bytes(config_eff, None, None).unwrap(),
            config_eff
        );
    }

    #[test]
    fn format_data_size_roundtrip() {
        assert_eq!(format_data_size(100 * 1024 * 1024), "100M");
        assert_eq!(format_data_size(1024 * 1024 * 1024), "1G");
        assert_eq!(format_data_size(512), "512B");
    }

    #[test]
    fn maddy_conf_parses_max_message_size() {
        let content = r#"
storage.imapsql sqlite:///tmp/x.db {
    appendlimit 100M
}
submission tcp://0.0.0.0:587 {
    max_message_size 80M
}
"#;
        let cfg = crate::maddy::parse_maddy_config(content).unwrap();
        assert_eq!(cfg.appendlimit.as_deref(), Some("100M"));
        assert_eq!(cfg.max_message_size.as_deref(), Some("80M"));
        assert_eq!(effective_max_message_bytes(&cfg), 80 * 1024 * 1024);
    }

    #[test]
    fn effective_max_message_bytes_uses_min_of_append_and_smtp() {
        let cfg = crate::AppConfig {
            appendlimit: Some("50M".into()),
            max_message_size: Some("32M".into()),
            ..Default::default()
        };
        assert_eq!(
            effective_max_message_bytes(&cfg),
            32 * 1024 * 1024
        );
    }
}
