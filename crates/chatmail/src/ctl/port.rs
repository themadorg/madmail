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

use super::context::CtlContext;

struct PortSpec {
    display: &'static str,
    port_key: &'static str,
    default_port: &'static str,
    local_keys: &'static [&'static str],
}

const PORT_SPECS: &[PortSpec] = &[
    PortSpec {
        display: "SMTP (25)",
        port_key: settings_keys::SMTP_PORT,
        default_port: "25",
        local_keys: &[settings_keys::SMTP_LOCAL_ONLY],
    },
    PortSpec {
        display: "Submission (587)",
        port_key: settings_keys::SUBMISSION_PORT,
        default_port: "587",
        local_keys: &[settings_keys::SUBMISSION_LOCAL_ONLY],
    },
    PortSpec {
        display: "Submission TLS (465)",
        port_key: settings_keys::SUBMISSION_TLS_PORT,
        default_port: "465",
        local_keys: &[settings_keys::SUBMISSION_TLS_LOCAL_ONLY],
    },
    PortSpec {
        display: "IMAP (143)",
        port_key: settings_keys::IMAP_PORT,
        default_port: "143",
        local_keys: &[settings_keys::IMAP_LOCAL_ONLY],
    },
    PortSpec {
        display: "IMAP TLS (993)",
        port_key: settings_keys::IMAP_TLS_PORT,
        default_port: "993",
        local_keys: &[settings_keys::IMAP_TLS_LOCAL_ONLY],
    },
    PortSpec {
        display: "TURN (3478)",
        port_key: settings_keys::TURN_PORT,
        default_port: "3478",
        local_keys: &[settings_keys::TURN_LOCAL_ONLY],
    },
    PortSpec {
        display: "SASL (24)",
        port_key: settings_keys::SASL_PORT,
        default_port: "24",
        local_keys: &[settings_keys::SASL_LOCAL_ONLY],
    },
    PortSpec {
        display: "Iroh (3340)",
        port_key: settings_keys::IROH_PORT,
        default_port: "3340",
        local_keys: &[settings_keys::IROH_LOCAL_ONLY],
    },
    PortSpec {
        display: "Shadowsocks (8388)",
        port_key: settings_keys::SS_PORT,
        default_port: "8388",
        local_keys: &[],
    },
    PortSpec {
        display: "HTTP (80)",
        port_key: settings_keys::HTTP_PORT,
        default_port: "80",
        local_keys: &[settings_keys::HTTP_LOCAL_ONLY],
    },
    PortSpec {
        display: "HTTPS (443)",
        port_key: settings_keys::HTTPS_PORT,
        default_port: "443",
        local_keys: &[settings_keys::HTTPS_LOCAL_ONLY],
    },
];

pub async fn port(args: &Args, cmd: &PortCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let settings = ctx.load_settings_map().await?;

    match cmd {
        PortCommand::Status => port_status_all(&settings),
        PortCommand::Smtp(c) => port_service(&pool, &settings, &PORT_SPECS[0], c).await,
        PortCommand::Submission(c) => port_service(&pool, &settings, &PORT_SPECS[1], c).await,
        PortCommand::SubmissionTls(c) => port_service(&pool, &settings, &PORT_SPECS[2], c).await,
        PortCommand::Imap(c) => port_service(&pool, &settings, &PORT_SPECS[3], c).await,
        PortCommand::ImapTls(c) => port_service(&pool, &settings, &PORT_SPECS[4], c).await,
        PortCommand::Turn(c) => port_service(&pool, &settings, &PORT_SPECS[5], c).await,
        PortCommand::Sasl(c) => port_service(&pool, &settings, &PORT_SPECS[6], c).await,
        PortCommand::Iroh(c) => port_service(&pool, &settings, &PORT_SPECS[7], c).await,
        PortCommand::Shadowsocks(c) => port_service(&pool, &settings, &PORT_SPECS[8], c).await,
        PortCommand::Http(c) => port_service(&pool, &settings, &PORT_SPECS[9], c).await,
        PortCommand::Https(c) => port_service(&pool, &settings, &PORT_SPECS[10], c).await,
    }
}

fn port_status_all(settings: &HashMap<String, String>) -> Result<()> {
    println!();
    for spec in PORT_SPECS {
        let mode = service_mode(settings, spec);
        let port = service_port(settings, spec);
        println!("  {:<24} port={port} mode={mode}", format!("{}:", spec.display));
    }
    println!();
    println!("  Note: restart service after changes.");
    println!();
    Ok(())
}

async fn port_service(
    pool: &chatmail_db::DbPool,
    settings: &HashMap<String, String>,
    spec: &PortSpec,
    cmd: &PortServiceCommand,
) -> Result<()> {
    match cmd {
        PortServiceCommand::Status => {
            println!();
            println!("  {}:", spec.display);
            println!("    port: {}", service_port(settings, spec));
            println!("    mode: {}", service_mode(settings, spec));
            println!();
            Ok(())
        }
        PortServiceCommand::Set { port } => {
            let p: u16 = port
                .parse()
                .map_err(|_| ChatmailError::config(format!("invalid port {port:?} (must be 1-65535)")))?;
            if !(1..=65535).contains(&p) {
                return Err(ChatmailError::config(format!(
                    "invalid port {port:?} (must be 1-65535)"
                )));
            }
            set_setting(pool, spec.port_key, &p.to_string()).await?;
            println!(
                "✅ {} port set to {p} (restart required — run: chatmail reload)",
                spec.display
            );
            Ok(())
        }
        PortServiceCommand::Reset => {
            delete_setting(pool, spec.port_key).await?;
            println!(
                "✅ {} port reset to config/default (restart required — run: chatmail reload)",
                spec.display
            );
            Ok(())
        }
        PortServiceCommand::Local => set_mode(pool, spec, "local").await,
        PortServiceCommand::Public => set_mode(pool, spec, "public").await,
    }
}

async fn set_mode(
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
    println!(
        "✅ {} set to {mode} (restart required — run: chatmail reload)",
        spec.display
    );
    Ok(())
}

fn service_port(settings: &HashMap<String, String>, spec: &PortSpec) -> String {
    if let Some(v) = settings.get(spec.port_key).filter(|s| !s.trim().is_empty()) {
        format!("{v} (override)")
    } else {
        format!("{} (default)", spec.default_port)
    }
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
