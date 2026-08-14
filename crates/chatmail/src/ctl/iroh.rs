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

//! `madmail iroh` — Iroh relay + IMAP discovery (`__IROH_*__`).

use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use chatmail_config::cli::IrohCommand;
use chatmail_config::{update_config_iroh_enable, Args};
use chatmail_db::{get_bool_setting, get_setting, set_setting, settings_keys};
use chatmail_types::{ChatmailError, Result};
use serde_json::{json, Value};

use super::context::CtlContext;
use super::output::CtlOut;

const IROH_NOT_CONFIGURED: &str =
    "Iroh is not configured in the config file (set iroh_relay_url in an imap block, or run: madmail iroh install)";

const RELOAD_HINT: &str = "Apply to a running server: madmail reload";

/// Windows releases do not ship `iroh-relay.exe`; enabling Iroh in config can fail service boot.
#[cfg(windows)]
const IROH_WINDOWS_UNAVAILABLE: &str =
    "Iroh relay is not available on Windows builds (no iroh-relay.exe packaged yet). \
status and disable still work; install/enable are Unix-only until Iroh is packaged for Windows.";

/// Default URL written by `madmail install --enable-iroh`.
const DEFAULT_IROH_RELAY_URL_TEMPLATE: &str = "http://$(public_ip):3340";

pub async fn iroh(args: &Args, cmd: Option<&IrohCommand>) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        None | Some(IrohCommand::Status) => status(args, &ctx, &pool).await,
        Some(IrohCommand::Install) => {
            reject_windows_iroh_runtime()?;
            install_cmd(args, &ctx, &pool).await
        }
        Some(IrohCommand::Enable) => {
            reject_windows_iroh_runtime()?;
            set_enabled(args, &ctx, &pool, true).await
        }
        Some(IrohCommand::Disable) => set_enabled(args, &ctx, &pool, false).await,
    }
}

/// Refuse install/enable on Windows so operators cannot brick a service start path.
fn reject_windows_iroh_runtime() -> Result<()> {
    #[cfg(windows)]
    {
        return Err(ChatmailError::config(IROH_WINDOWS_UNAVAILABLE));
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

async fn status(args: &Args, ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<()> {
    let out = CtlOut::from_args(args, "iroh status");
    let data = iroh_status_data(ctx, pool).await?;

    if out.is_json() {
        return out.emit(data);
    }

    out.blank();
    out.line("  Iroh relay + IMAP discovery");
    out.line(format!(
        "  Configured:     {}",
        yn(data["configured"].as_bool().unwrap_or(false))
    ));
    out.line(format!(
        "  Admin enabled:  {}",
        yn(data["admin_enabled"].as_bool().unwrap_or(false))
    ));
    out.line(format!(
        "  Runtime active: {}",
        yn(data["runtime_enabled"].as_bool().unwrap_or(false))
    ));
    out.line(format!(
        "  Port:           {}",
        data["port"].as_u64().unwrap_or(3340)
    ));
    out.line(format!(
        "  Local only:     {}",
        yn(data["local_only"].as_bool().unwrap_or(false))
    ));
    if let Some(url) = data["relay_url"].as_str().filter(|s| !s.is_empty()) {
        out.line(format!("  Relay URL:      {url}"));
    } else {
        out.line("  Relay URL:      (none)");
    }
    out.line(format!(
        "  Listen probe:   {}",
        if data["listening"].as_bool() == Some(true) {
            "port open (localhost)"
        } else if data["runtime_enabled"].as_bool() == Some(true) {
            "not reachable on localhost (server may be down or bound elsewhere)"
        } else {
            "n/a (not active)"
        }
    ));
    if !data["configured"].as_bool().unwrap_or(false) {
        out.line(format!("  ({IROH_NOT_CONFIGURED})"));
    }
    out.blank();
    out.line(format!("  {RELOAD_HINT}"));
    out.line("  Port / bind mode: madmail port iroh …");
    out.blank();
    Ok(())
}

async fn install_cmd(args: &Args, ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<()> {
    let out = CtlOut::from_args(args, "iroh install");
    let config_path = &args.config;
    if !config_path.is_file() {
        return Err(ChatmailError::config(format!(
            "config file not found: {} — pass --config or create a config first",
            config_path.display()
        )));
    }

    let already = ctx.config.iroh_configured();
    let relay_url = default_relay_url_for_install(&ctx.config);

    if !already {
        update_config_iroh_enable(config_path, &relay_url)?;
    }

    set_setting(pool, settings_keys::IROH_ENABLED, "true").await?;

    // Re-read config so status fields in JSON reflect the write.
    let reloaded = CtlContext::from_args(args)?;
    let configured = reloaded.config.iroh_configured();
    let effective = reloaded
        .config
        .effective_iroh_relay_url(reloaded.config.hostname.as_deref().unwrap_or("127.0.0.1"))
        .unwrap_or_else(|| relay_url.clone());

    let human = if already {
        format!("✅ Iroh already in config; admin toggle enabled ({RELOAD_HINT})")
    } else {
        format!(
            "✅ Iroh installed in {} (relay_url={effective}; {RELOAD_HINT})",
            config_path.display()
        )
    };

    out.done_msg(
        human,
        json!({
            "configured": configured,
            "config_updated": !already,
            "enabled": true,
            "relay_url": effective,
            "reload_required": true,
        }),
        if already {
            "Iroh admin toggle enabled"
        } else {
            "Iroh installed and enabled"
        },
    )
}

async fn set_enabled(
    args: &Args,
    ctx: &CtlContext,
    pool: &chatmail_db::DbPool,
    on: bool,
) -> Result<()> {
    if on && !ctx.config.iroh_configured() {
        return Err(ChatmailError::config(IROH_NOT_CONFIGURED));
    }

    let out = CtlOut::from_args(args, if on { "iroh enable" } else { "iroh disable" });
    set_setting(
        pool,
        settings_keys::IROH_ENABLED,
        if on { "true" } else { "false" },
    )
    .await?;
    let msg = if on { "Iroh enabled" } else { "Iroh disabled" };
    out.done_msg(
        if on {
            format!("✅ Iroh enabled ({RELOAD_HINT})")
        } else {
            format!("🚫 Iroh disabled ({RELOAD_HINT})")
        },
        json!({ "enabled": on, "reload_required": true }),
        msg,
    )
}

async fn iroh_status_data(ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<Value> {
    let configured = ctx.config.iroh_configured();
    // Default true matches iroh_boot / admin when unset.
    let admin_enabled = get_bool_setting(pool, settings_keys::IROH_ENABLED, true).await?;
    let runtime_enabled = configured && admin_enabled;

    let port = effective_port(pool, &ctx.config).await?;
    let local_only = effective_local_only(pool).await?;
    let hostname = ctx
        .config
        .hostname
        .as_deref()
        .or(ctx.config.public_ip.as_deref())
        .unwrap_or("127.0.0.1");
    let relay_url = if configured {
        effective_relay_url(pool, &ctx.config, hostname).await?
    } else {
        String::new()
    };

    let listening = if runtime_enabled {
        probe_listening(port, local_only)
    } else {
        false
    };

    Ok(json!({
        "configured": configured,
        "admin_enabled": admin_enabled,
        "runtime_enabled": runtime_enabled,
        "port": port,
        "local_only": local_only,
        "relay_url": relay_url,
        "listening": listening,
        "reload_required": false,
    }))
}

async fn effective_port(
    pool: &chatmail_db::DbPool,
    cfg: &chatmail_config::AppConfig,
) -> Result<u16> {
    if let Ok(Some(v)) = get_setting(pool, settings_keys::IROH_PORT).await {
        if let Ok(p) = v.trim().parse::<u16>() {
            if p != 0 {
                return Ok(p);
            }
        }
    }
    Ok(if cfg.iroh_port == 0 {
        3340
    } else {
        cfg.iroh_port
    })
}

async fn effective_local_only(pool: &chatmail_db::DbPool) -> Result<bool> {
    if let Ok(Some(v)) = get_setting(pool, settings_keys::IROH_LOCAL_ONLY).await {
        return Ok(v == "true");
    }
    Ok(false)
}

async fn effective_relay_url(
    pool: &chatmail_db::DbPool,
    cfg: &chatmail_config::AppConfig,
    hostname: &str,
) -> Result<String> {
    if let Ok(Some(v)) = get_setting(pool, settings_keys::IROH_RELAY_URL).await {
        if !v.trim().is_empty() {
            return Ok(v);
        }
    }
    Ok(cfg.effective_iroh_relay_url(hostname).unwrap_or_default())
}

fn default_relay_url_for_install(cfg: &chatmail_config::AppConfig) -> String {
    if let Some(url) = cfg.iroh_relay_url.as_ref().filter(|s| !s.is_empty()) {
        return url.clone();
    }
    if let Some(url) = cfg.effective_iroh_relay_url(
        cfg.hostname
            .as_deref()
            .or(cfg.public_ip.as_deref())
            .unwrap_or("127.0.0.1"),
    ) {
        return url;
    }
    // Match install --enable-iroh template when public_ip is a maddy variable.
    DEFAULT_IROH_RELAY_URL_TEMPLATE.to_string()
}

/// Best-effort check: can we open TCP to the Iroh listen port on loopback?
fn probe_listening(port: u16, _local_only: bool) -> bool {
    // Probe loopback only (v4 + v6). Dual-stack / unbound listeners still accept on localhost.
    let candidates = [
        SocketAddr::from(([127, 0, 0, 1], port)),
        SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port)),
    ];
    for addr in candidates {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return true;
        }
    }
    false
}

fn yn(v: bool) -> &'static str {
    if v {
        "yes"
    } else {
        "no"
    }
}
