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

//! Persist the opt-in metrics listener without changing other server settings.

use std::path::Path;

use chatmail_types::{ChatmailError, Result};

const DEFAULT_LISTEN: &str = "127.0.0.1:9749";

#[derive(Default)]
pub struct MonitorOptions<'a> {
    pub ip: Option<std::net::IpAddr>,
    pub port: Option<u16>,
    pub username: Option<&'a str>,
    pub password: Option<&'a str>,
    pub clear_password: bool,
}

pub fn update_config_monitor(path: &Path, enabled: bool) -> Result<bool> {
    update_config_monitor_options(path, enabled, &MonitorOptions::default())
}

pub fn update_config_monitor_options(
    path: &Path,
    enabled: bool,
    options: &MonitorOptions<'_>,
) -> Result<bool> {
    let config = crate::load_config(path)?;
    if options.port == Some(0) {
        return Err(ChatmailError::config(
            "metrics port must be between 1 and 65535",
        ));
    }
    let mut desired = config.clone();
    if enabled {
        let listen = config
            .openmetrics_listen
            .as_deref()
            .unwrap_or(DEFAULT_LISTEN);
        desired.openmetrics_listen = Some(if options.ip.is_some() || options.port.is_some() {
            let (host, port) = listen
                .rsplit_once(':')
                .ok_or_else(|| ChatmailError::config("invalid metrics listen address"))?;
            let host = options
                .ip
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| host.trim_matches(['[', ']']).to_string());
            let port = options
                .port
                .map(|p| p.to_string())
                .unwrap_or_else(|| port.to_string());
            if host.contains(':') {
                format!("[{host}]:{port}")
            } else {
                format!("{host}:{port}")
            }
        } else {
            listen.to_string()
        });
        if let Some(username) = options.username {
            if username.is_empty()
                || username.contains(':')
                || username.chars().any(char::is_control)
            {
                return Err(ChatmailError::config(
                    "metrics username must be nonempty and contain no colon or control characters",
                ));
            }
            desired.openmetrics_username = Some(username.to_string());
        }
        if options.clear_password {
            desired.openmetrics_password = None;
        } else if let Some(password) = options.password {
            if password.is_empty() || password.chars().any(char::is_control) {
                return Err(ChatmailError::config(
                    "metrics password must be nonempty and contain no control characters",
                ));
            }
            desired.openmetrics_password = Some(password.to_string());
        }
    } else {
        desired.openmetrics_listen = None;
        desired.openmetrics_username = None;
        desired.openmetrics_password = None;
    }
    if desired == config {
        return Ok(false);
    }
    let raw = std::fs::read_to_string(path)?;
    let is_toml = path.extension().and_then(|e| e.to_str()) == Some("toml");
    let updated = if is_toml {
        let mut doc: toml::Table = toml::from_str(&raw)
            .map_err(|e| ChatmailError::config(format!("invalid TOML: {e}")))?;
        for (key, value) in [
            ("openmetrics_listen", &desired.openmetrics_listen),
            ("openmetrics_username", &desired.openmetrics_username),
            ("openmetrics_password", &desired.openmetrics_password),
        ] {
            if let Some(value) = value {
                doc.insert(key.into(), toml::Value::String(value.clone()));
            } else {
                doc.remove(key);
            }
        }
        toml::to_string_pretty(&doc)
            .map_err(|e| ChatmailError::config(format!("serialize TOML: {e}")))?
    } else {
        let mut updated = disable_maddy(&raw)?;
        if let Some(listen) = &desired.openmetrics_listen {
            updated.push_str(&format!("\nopenmetrics tcp://{listen} {{\n"));
            for (key, value) in [
                ("username", &desired.openmetrics_username),
                ("password", &desired.openmetrics_password),
            ] {
                if let Some(value) = value {
                    // Quote values using the config lexer's escape syntax.
                    let value = value.replace('"', "\\\"");
                    updated.push_str(&format!("    {key} \"{value}\"\n"));
                }
            }
            updated.push_str("}\n");
        }
        updated
    };
    if !is_toml {
        let parsed = crate::parse_maddy_config(&updated)
            .map_err(|e| ChatmailError::config(e.to_string()))?;
        if parsed != desired {
            return Err(ChatmailError::config(
                "could not safely update metrics configuration; edit it manually",
            ));
        }
    }
    std::fs::write(path, updated)?;
    Ok(true)
}

fn disable_maddy(raw: &str) -> Result<String> {
    let ast = crate::read_maddy_ast(raw).map_err(|e| ChatmailError::config(e.to_string()))?;
    let tokens = crate::madmail_lexer::lex_all(raw);
    let mut disabled = std::collections::HashSet::new();
    for node in ast.nodes.iter().filter(|n| n.name == "openmetrics") {
        let start = node.line;
        let mut end = start;
        if node.children.is_some() {
            let mut depth = 0;
            let mut opened = false;
            for token in tokens.iter().filter(|t| t.line >= start) {
                if token.text == "{" {
                    depth += 1;
                    opened = true;
                } else if token.text == "}" {
                    depth -= 1;
                    if opened && depth == 0 {
                        end = token.line;
                        break;
                    }
                }
            }
        }
        disabled.extend(start..=end);
    }
    Ok(raw
        .split_inclusive('\n')
        .enumerate()
        .map(|(i, line)| {
            if disabled.contains(&((i + 1) as u32)) {
                String::new()
            } else {
                line.to_string()
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_both_config_formats_and_is_idempotent() {
        for (name, initial) in [
            ("madmail.conf", "hostname mail.example.org\n"),
            ("chatmail.toml", "hostname = \"mail.example.org\"\n"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(name);
            std::fs::write(&path, initial).unwrap();
            assert!(update_config_monitor(&path, true).unwrap());
            assert_eq!(
                crate::load_config(&path)
                    .unwrap()
                    .openmetrics_listen
                    .as_deref(),
                Some(DEFAULT_LISTEN)
            );
            assert!(!update_config_monitor(&path, true).unwrap());
            assert!(update_config_monitor(&path, false).unwrap());
            assert_eq!(crate::load_config(&path).unwrap().openmetrics_listen, None);
            assert!(!update_config_monitor(&path, false).unwrap());
            assert_eq!(
                crate::load_config(&path).unwrap().hostname.as_deref(),
                Some("mail.example.org")
            );
        }
    }

    #[test]
    fn custom_listener_and_password_round_trip_and_can_be_cleared() {
        for name in ["madmail.conf", "chatmail.toml"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(name);
            std::fs::write(&path, "").unwrap();
            let options = MonitorOptions {
                ip: Some("::1".parse().unwrap()),
                port: Some(19876),
                username: Some("exporter"),
                password: Some("secret \"quoted\" \\ # {}"),
                clear_password: false,
            };
            assert!(update_config_monitor_options(&path, true, &options).unwrap());
            let config = crate::load_config(&path).unwrap();
            assert_eq!(config.openmetrics_listen.as_deref(), Some("[::1]:19876"));
            assert_eq!(config.openmetrics_password.as_deref(), options.password);
            assert_eq!(config.openmetrics_username.as_deref(), Some("exporter"));
            assert!(!update_config_monitor_options(&path, true, &options).unwrap());
            let options = MonitorOptions {
                ip: Some("0.0.0.0".parse().unwrap()),
                ..Default::default()
            };
            update_config_monitor_options(&path, true, &options).unwrap();
            let config = crate::load_config(&path).unwrap();
            assert_eq!(config.openmetrics_listen.as_deref(), Some("0.0.0.0:19876"));
            assert!(config.openmetrics_password.is_some());
            update_config_monitor_options(
                &path,
                true,
                &MonitorOptions {
                    clear_password: true,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                crate::load_config(&path).unwrap().openmetrics_password,
                None
            );
            for options in [
                MonitorOptions {
                    password: Some(""),
                    ..Default::default()
                },
                MonitorOptions {
                    password: Some("bad\nvalue"),
                    ..Default::default()
                },
                MonitorOptions {
                    username: Some("bad:user"),
                    ..Default::default()
                },
                MonitorOptions {
                    port: Some(0),
                    ..Default::default()
                },
            ] {
                let before = std::fs::read(&path).unwrap();
                assert!(update_config_monitor_options(&path, true, &options).is_err());
                assert_eq!(std::fs::read(&path).unwrap(), before);
            }
            update_config_monitor(&path, false).unwrap();
            assert_eq!(
                crate::load_config(&path).unwrap().openmetrics_username,
                None
            );
        }
    }

    #[test]
    fn disables_all_metrics_blocks_preserving_other_content() {
        let raw = "# openmetrics is optional\nhostname mail.example.org\nopenmetrics tcp://127.0.0.1:1234 { }\nopenmetrics tcp://127.0.0.1:5678 {\n # comment with }\n}\nimap tcp://127.0.0.1:143 {\n}\n";
        let updated = disable_maddy(raw).unwrap();
        assert_eq!(
            crate::parse_maddy_config(&updated)
                .unwrap()
                .openmetrics_listen,
            None
        );
        assert!(updated.contains("imap tcp://127.0.0.1:143 {\n}\n"));
        assert!(updated.starts_with("# openmetrics is optional\nhostname mail.example.org\n"));
    }
}
