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

//! Flexible enable/disable string parsing for config files and DB settings.
//!
//! Operators may write any common form; **disable / off is the safe default**
//! for unknown or empty values (aligned with No-Log and other privacy defaults).

/// Case-insensitive truthy tokens accepted for enable-style flags (`debug yes`, etc.).
const TRUTHY: &[&str] = &["1", "true", "yes", "y", "on", "enable", "enabled", "t"];

/// Case-insensitive falsy tokens accepted for disable-style flags (`debug no`, etc.).
const FALSY: &[&str] = &["0", "false", "no", "n", "off", "disable", "disabled", "f"];

/// Returns `true` when `s` is a recognized enable value (`true`, `yes`, `1`, `enable`, …).
///
/// Matching is case-insensitive and trims surrounding whitespace.
/// Empty and unknown strings are **not** truthy (safe default: disabled).
pub fn is_truthy(s: &str) -> bool {
    let v = s.trim().to_ascii_lowercase();
    TRUTHY.iter().any(|t| *t == v)
}

/// Returns `true` when `s` is a recognized disable value (`false`, `no`, `0`, `disable`, …).
///
/// Empty string is treated as falsy. Unknown non-empty strings are neither truthy nor falsy.
pub fn is_falsy(s: &str) -> bool {
    let v = s.trim().to_ascii_lowercase();
    v.is_empty() || FALSY.iter().any(|t| *t == v)
}

/// Parse a config / settings boolean string.
///
/// Accepts common enable forms: `True`, `true`, `yes`, `1`, `on`, `enable`, `enabled`, …
/// Accepts common disable forms: `False`, `false`, `no`, `0`, `off`, `disable`, `disabled`, …
///
/// Anything not recognized as enable is treated as **disabled** (including empty and unknown).
/// This matches Madmail’s preference for safe defaults (e.g. No-Log when logging is not opted in).
pub fn parse_bool_str(s: &str) -> bool {
    is_truthy(s)
}

/// Like [`parse_bool_str`], but returns `None` when the value is neither a known
/// enable nor disable token (so callers can keep an existing default).
pub fn parse_bool_str_opt(s: &str) -> Option<bool> {
    if is_truthy(s) {
        Some(true)
    } else if is_falsy(s) {
        Some(false)
    } else {
        None
    }
}

/// Serde helper for optional bool fields that also accept string/int forms
/// (`"yes"`, `"enable"`, `1`, …) in TOML.
pub fn deserialize_option_bool_flexible<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Flex {
        Bool(bool),
        Str(String),
        Int(i64),
    }

    match Option::<Flex>::deserialize(deserializer)? {
        None => Ok(None),
        Some(Flex::Bool(b)) => Ok(Some(b)),
        Some(Flex::Str(s)) => Ok(Some(parse_bool_str(&s))),
        Some(Flex::Int(n)) => Ok(Some(n != 0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthy_forms() {
        for s in [
            "1", "true", "True", "TRUE", " yes ", "YES", "y", "on", "On", "enable", "Enable",
            "enabled", "ENABLED", "t",
        ] {
            assert!(is_truthy(s), "expected truthy: {s:?}");
            assert!(parse_bool_str(s), "expected parse true: {s:?}");
            assert_eq!(parse_bool_str_opt(s), Some(true));
        }
    }

    #[test]
    fn falsy_forms() {
        for s in [
            "0", "false", "False", "FALSE", " no ", "NO", "n", "off", "Off", "disable", "Disable",
            "disabled", "DISABLED", "f", "", "   ",
        ] {
            assert!(is_falsy(s), "expected falsy: {s:?}");
            assert!(!parse_bool_str(s), "expected parse false: {s:?}");
            assert_eq!(parse_bool_str_opt(s), Some(false));
        }
    }

    #[test]
    fn unknown_is_disabled() {
        assert!(!parse_bool_str("maybe"));
        assert!(!parse_bool_str("2"));
        assert_eq!(parse_bool_str_opt("maybe"), None);
        assert_eq!(parse_bool_str_opt("2"), None);
    }
}
