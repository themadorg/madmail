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

//! `chatmail port` — Madmail `ctl/port.go`.

use std::collections::HashMap;

use chatmail_config::cli::{PortCommand, PortServiceCommand};
use chatmail_config::Args;
use chatmail_db::{delete_setting, set_setting, settings_keys};
use chatmail_types::{ChatmailError, Result};
use serde::Serialize;

use super::context::CtlContext;
use super::output::CtlOut;

struct PortSpec {
    name: &'static str,
    display: &'static str,
    port_key: &'static str,
    default_port: &'static str,
    local_keys: &'static [&'static str],
    /// `__HTTP_ENABLED__` / `__HTTPS_ENABLED__` (HTTP/HTTPS only).
    enabled_key: Option<&'static str>,
}

const PORT_SPECS: &[PortSpec] = &[
    PortSpec {
        name: "smtp",
        display: "SMTP (25)",
        port_key: settings_keys::SMTP_PORT,
        default_port: "25",
        local_keys: &[settings_keys::SMTP_LOCAL_ONLY],
        enabled_key: None,
    },
    PortSpec {
        name: "submission",
        display: "Submission (587)",
        port_key: settings_keys::SUBMISSION_PORT,
        default_port: "587",
        local_keys: &[settings_keys::SUBMISSION_LOCAL_ONLY],
        enabled_key: None,
    },
    PortSpec {
        name: "submission-tls",
        display: "Submission TLS (465)",
        port_key: settings_keys::SUBMISSION_TLS_PORT,
        default_port: "465",
        local_keys: &[settings_keys::SUBMISSION_TLS_LOCAL_ONLY],
        enabled_key: None,
    },
    PortSpec {
        name: "imap",
        display: "IMAP (143)",
        port_key: settings_keys::IMAP_PORT,
        default_port: "143",
        local_keys: &[settings_keys::IMAP_LOCAL_ONLY],
        enabled_key: None,
    },
    PortSpec {
        name: "imap-tls",
        display: "IMAP TLS (993)",
        port_key: settings_keys::IMAP_TLS_PORT,
        default_port: "993",
        local_keys: &[settings_keys::IMAP_TLS_LOCAL_ONLY],
        enabled_key: None,
    },
    PortSpec {
        name: "turn",
        display: "TURN (3478)",
        port_key: settings_keys::TURN_PORT,
        default_port: "3478",
        local_keys: &[settings_keys::TURN_LOCAL_ONLY],
        enabled_key: None,
    },
    PortSpec {
        name: "sasl",
        display: "SASL (24)",
        port_key: settings_keys::SASL_PORT,
        default_port: "24",
        local_keys: &[settings_keys::SASL_LOCAL_ONLY],
        enabled_key: None,
    },
    PortSpec {
        name: "iroh",
        display: "Iroh (3340)",
        port_key: settings_keys::IROH_PORT,
        default_port: "3340",
        local_keys: &[settings_keys::IROH_LOCAL_ONLY],
        enabled_key: None,
    },
    PortSpec {
        name: "shadowsocks",
        display: "Shadowsocks (8388)",
        port_key: settings_keys::SS_PORT,
        default_port: "8388",
        local_keys: &[],
        enabled_key: None,
    },
    PortSpec {
        name: "http",
        display: "HTTP (80)",
        port_key: settings_keys::HTTP_PORT,
        default_port: "80",
        local_keys: &[settings_keys::HTTP_LOCAL_ONLY],
        enabled_key: Some(settings_keys::HTTP_ENABLED),
    },
    PortSpec {
        name: "https",
        display: "HTTPS (443)",
        port_key: settings_keys::HTTPS_PORT,
        default_port: "443",
        local_keys: &[settings_keys::HTTPS_LOCAL_ONLY],
        enabled_key: Some(settings_keys::HTTPS_ENABLED),
    },
];

#[derive(Serialize)]
struct PortServiceStatus {
    name: &'static str,
    port: String,
    mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
}

pub async fn port(args: &Args, cmd: &PortCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let settings = ctx.load_settings_map().await?;

    match cmd {
        PortCommand::Status => port_status_all(args, &settings),
        PortCommand::Smtp(c) => port_service(args, &pool, &settings, &PORT_SPECS[0], c).await,
        PortCommand::Submission(c) => port_service(args, &pool, &settings, &PORT_SPECS[1], c).await,
        PortCommand::SubmissionTls(c) => {
            port_service(args, &pool, &settings, &PORT_SPECS[2], c).await
        }
        PortCommand::Imap(c) => port_service(args, &pool, &settings, &PORT_SPECS[3], c).await,
        PortCommand::ImapTls(c) => port_service(args, &pool, &settings, &PORT_SPECS[4], c).await,
        PortCommand::Turn(c) => port_service(args, &pool, &settings, &PORT_SPECS[5], c).await,
        PortCommand::Sasl(c) => port_service(args, &pool, &settings, &PORT_SPECS[6], c).await,
        PortCommand::Iroh(c) => port_service(args, &pool, &settings, &PORT_SPECS[7], c).await,
        PortCommand::Shadowsocks(c) => {
            port_service(args, &pool, &settings, &PORT_SPECS[8], c).await
        }
        PortCommand::Http(c) => port_service(args, &pool, &settings, &PORT_SPECS[9], c).await,
        PortCommand::Https(c) => port_service(args, &pool, &settings, &PORT_SPECS[10], c).await,
    }
}

fn port_status_all(args: &Args, settings: &HashMap<String, String>) -> Result<()> {
    let out = CtlOut::from_args(args, "port status");
    if out.is_json() {
        let services: Vec<PortServiceStatus> = PORT_SPECS
            .iter()
            .map(|spec| PortServiceStatus {
                name: spec.name,
                port: service_port_value(settings, spec),
                mode: service_mode(settings, spec),
                enabled: service_enabled(settings, spec),
            })
            .collect();
        return out.emit(serde_json::json!({ "services": services }));
    }
    out.blank();
    for spec in PORT_SPECS {
        let mode = service_mode(settings, spec);
        let port = service_port_display(settings, spec);
        let mut line = format!(
            "  {:<24} port={port} mode={mode}",
            format!("{}:", spec.display)
        );
        if let Some(on) = service_enabled(settings, spec) {
            line.push_str(&format!(
                " enabled={}",
                if on { "yes" } else { "no" }
            ));
        }
        out.line(line);
    }
    out.blank();
    out.line("  Note: restart service after changes.");
    out.blank();
    Ok(())
}

async fn port_service(
    args: &Args,
    pool: &chatmail_db::DbPool,
    settings: &HashMap<String, String>,
    spec: &PortSpec,
    cmd: &PortServiceCommand,
) -> Result<()> {
    let out = CtlOut::from_args(args, "port");
    match cmd {
        PortServiceCommand::Status => {
            let enabled = service_enabled(settings, spec);
            if out.is_json() {
                let mut data = serde_json::json!({
                    "name": spec.name,
                    "port": service_port_value(settings, spec),
                    "mode": service_mode(settings, spec),
                });
                if let Some(on) = enabled {
                    data["enabled"] = serde_json::json!(on);
                }
                out.emit(data)
            } else {
                out.blank();
                out.line(format!("  {}:", spec.display));
                out.line(format!(
                    "    port: {}",
                    service_port_display(settings, spec)
                ));
                out.line(format!("    mode: {}", service_mode(settings, spec)));
                if let Some(on) = enabled {
                    out.line(format!(
                        "    enabled: {}",
                        if on { "yes" } else { "no" }
                    ));
                }
                out.blank();
                Ok(())
            }
        }
        PortServiceCommand::Set { port } => {
            let p: u16 = port.parse().map_err(|_| {
                ChatmailError::config(format!("invalid port {port:?} (must be 1-65535)"))
            })?;
            if !(1..=65535).contains(&p) {
                return Err(ChatmailError::config(format!(
                    "invalid port {port:?} (must be 1-65535)"
                )));
            }
            set_setting(pool, spec.port_key, &p.to_string()).await?;
            out.done_msg(
                format!(
                    "✅ {} port set to {p} (restart required — run: chatmail reload)",
                    spec.display
                ),
                serde_json::json!({ "name": spec.name, "port": p.to_string() }),
                format!("{} port set to {p}", spec.name),
            )
        }
        PortServiceCommand::Reset => {
            delete_setting(pool, spec.port_key).await?;
            out.done_msg(
                format!(
                    "✅ {} port reset to config/default (restart required — run: chatmail reload)",
                    spec.display
                ),
                serde_json::json!({ "name": spec.name, "reset": true }),
                format!("{} port reset", spec.name),
            )
        }
        PortServiceCommand::Local => set_mode(out, pool, spec, "local").await,
        PortServiceCommand::Public => set_mode(out, pool, spec, "public").await,
        PortServiceCommand::Enable => set_listener_enabled(out, pool, spec, true).await,
        PortServiceCommand::Disable => set_listener_enabled(out, pool, spec, false).await,
    }
}

async fn set_mode(
    out: CtlOut,
    pool: &chatmail_db::DbPool,
    spec: &PortSpec,
    mode: &str,
) -> Result<()> {
    if spec.local_keys.is_empty() {
        return Err(ChatmailError::config(format!(
            "{} does not support local/public mode",
            spec.display
        )));
    }
    match mode {
        "local" => {
            for key in spec.local_keys {
                set_setting(pool, key, "true").await?;
            }
        }
        "public" => {
            for key in spec.local_keys {
                delete_setting(pool, key).await?;
            }
        }
        _ => return Err(ChatmailError::config(format!("unsupported mode: {mode}"))),
    }
    out.done_msg(
        format!(
            "✅ {} set to {mode} (restart required — run: chatmail reload)",
            spec.display
        ),
        serde_json::json!({ "name": spec.name, "mode": mode }),
        format!("{} set to {mode}", spec.name),
    )
}

fn service_port_value(settings: &HashMap<String, String>, spec: &PortSpec) -> String {
    settings
        .get(spec.port_key)
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| spec.default_port.to_string())
}

fn service_port_display(settings: &HashMap<String, String>, spec: &PortSpec) -> String {
    if let Some(v) = settings.get(spec.port_key).filter(|s| !s.trim().is_empty()) {
        format!("{v} (override)")
    } else {
        format!("{} (default)", spec.default_port)
    }
}

async fn set_listener_enabled(
    out: CtlOut,
    pool: &chatmail_db::DbPool,
    spec: &PortSpec,
    on: bool,
) -> Result<()> {
    let key = spec.enabled_key.ok_or_else(|| {
        ChatmailError::config(format!(
            "{} does not support enable/disable",
            spec.display
        ))
    })?;
    set_setting(pool, key, if on { "true" } else { "false" }).await?;
    let verb = if on { "enabled" } else { "disabled" };
    out.done_msg(
        format!(
            "✅ {} {verb} (restart required — run: madmail reload)",
            spec.display
        ),
        serde_json::json!({
            "name": spec.name,
            "enabled": on,
            "reload_required": true,
        }),
        format!("{} {verb}", spec.name),
    )
}

fn service_enabled(settings: &HashMap<String, String>, spec: &PortSpec) -> Option<bool> {
    spec.enabled_key.map(|key| {
        settings
            .get(key)
            .map(|v| !v.eq_ignore_ascii_case("false"))
            .unwrap_or(true)
    })
}

fn service_mode(settings: &HashMap<String, String>, spec: &PortSpec) -> &'static str {
    if spec.local_keys.is_empty() {
        return "n/a";
    }
    for key in spec.local_keys {
        if settings
            .get(*key)
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            return "local";
        }
    }
    "public"
}
