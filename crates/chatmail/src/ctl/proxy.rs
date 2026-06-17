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

//! `madmail proxy` — Shadowsocks circumvention proxy (`__SS_*__`).

use chatmail_config::cli::{ProxyCommand, ProxySettingCommand};
use chatmail_config::Args;
use chatmail_db::{delete_setting, get_setting, set_setting, settings_keys};
use chatmail_shadowsocks::{parse_cipher, resolve_runtime};
use chatmail_types::{ChatmailError, Result};
use serde_json::{json, Value};

use super::context::CtlContext;
use super::output::CtlOut;

const SS_NOT_CONFIGURED: &str =
    "Shadowsocks is not configured in maddy.conf (set ss_addr and ss_password in the chatmail block)";

const RELOAD_HINT: &str = "Apply to a running server: madmail reload";

pub async fn proxy(args: &Args, cmd: Option<&ProxyCommand>) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        None | Some(ProxyCommand::Status) => status(args, &ctx, &pool).await,
        Some(ProxyCommand::Enable) => set_enabled(args, &ctx, &pool, true).await,
        Some(ProxyCommand::Disable) => set_enabled(args, &ctx, &pool, false).await,
        Some(ProxyCommand::Cipher { cmd }) => cipher_setting(args, &ctx, &pool, cmd.as_ref()).await,
        Some(ProxyCommand::Password { cmd }) => {
            password_setting(args, &ctx, &pool, cmd.as_ref()).await
        }
    }
}

async fn status(args: &Args, ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<()> {
    let out = CtlOut::from_args(args, "proxy status");
    let data = proxy_status_data(ctx, pool).await?;

    if out.is_json() {
        return out.emit(data);
    }

    out.blank();
    out.line("  Shadowsocks proxy (raw TCP)");
    out.line(format!(
        "  Configured:  {}",
        if data["configured"].as_bool().unwrap_or(false) {
            "yes"
        } else {
            "no"
        }
    ));
    if data["configured"].as_bool() == Some(true) {
        out.line(format!(
            "  Enabled:     {}",
            if data["enabled"].as_bool().unwrap_or(false) {
                "yes"
            } else {
                "no"
            }
        ));
        out.line(format!(
            "  Port:        {}",
            data["port"].as_str().unwrap_or("")
        ));
        out.line(format!(
            "  Cipher:      {} ({})",
            data["cipher"].as_str().unwrap_or(""),
            data["cipher_source"].as_str().unwrap_or("")
        ));
        out.line(format!(
            "  Password:    {}",
            if data["password_db_override"].as_bool().unwrap_or(false) {
                "DB override"
            } else {
                "config file"
            }
        ));
        if let Some(url) = data["shadowsocks_url"].as_str().filter(|s| !s.is_empty()) {
            out.line(format!("  Client URL:  {url}"));
        }
    } else {
        out.line("  Enabled:     no");
        out.line(format!("  ({SS_NOT_CONFIGURED})"));
    }
    out.blank();
    out.line(format!("  {RELOAD_HINT}"));
    out.blank();
    Ok(())
}

async fn set_enabled(
    args: &Args,
    ctx: &CtlContext,
    pool: &chatmail_db::DbPool,
    on: bool,
) -> Result<()> {
    if on && !ctx.config.ss_configured() {
        return Err(ChatmailError::config(SS_NOT_CONFIGURED));
    }

    let out = CtlOut::from_args(args, if on { "proxy enable" } else { "proxy disable" });
    set_setting(
        pool,
        settings_keys::SS_ENABLED,
        if on { "true" } else { "false" },
    )
    .await?;
    let msg = if on {
        "Shadowsocks proxy enabled"
    } else {
        "Shadowsocks proxy disabled"
    };
    out.done_msg(
        if on {
            format!("✅ Shadowsocks proxy enabled ({RELOAD_HINT})")
        } else {
            format!("🚫 Shadowsocks proxy disabled ({RELOAD_HINT})")
        },
        json!({ "enabled": on, "reload_required": true }),
        msg,
    )
}

async fn cipher_setting(
    args: &Args,
    ctx: &CtlContext,
    pool: &chatmail_db::DbPool,
    cmd: Option<&ProxySettingCommand>,
) -> Result<()> {
    match cmd {
        None | Some(ProxySettingCommand::Status) => {
            setting_status(
                args,
                ctx,
                pool,
                settings_keys::SS_CIPHER,
                "proxy cipher status",
                "cipher",
                ctx.config.ss_cipher.as_deref().unwrap_or("aes-128-gcm"),
            )
            .await
        }
        Some(ProxySettingCommand::Set { value }) => {
            setting_set(
                args,
                ctx,
                pool,
                settings_keys::SS_CIPHER,
                "proxy cipher set",
                value,
                validate_cipher,
            )
            .await
        }
        Some(ProxySettingCommand::Reset) => {
            setting_reset(
                args,
                ctx,
                pool,
                settings_keys::SS_CIPHER,
                "proxy cipher reset",
                |cfg| {
                    cfg.ss_cipher
                        .clone()
                        .unwrap_or_else(|| "aes-128-gcm".into())
                },
            )
            .await
        }
    }
}

async fn password_setting(
    args: &Args,
    ctx: &CtlContext,
    pool: &chatmail_db::DbPool,
    cmd: Option<&ProxySettingCommand>,
) -> Result<()> {
    match cmd {
        None | Some(ProxySettingCommand::Status) => password_status(args, ctx, pool).await,
        Some(ProxySettingCommand::Set { value }) => {
            setting_set(
                args,
                ctx,
                pool,
                settings_keys::SS_PASSWORD,
                "proxy password set",
                value,
                validate_password,
            )
            .await
        }
        Some(ProxySettingCommand::Reset) => {
            setting_reset(
                args,
                ctx,
                pool,
                settings_keys::SS_PASSWORD,
                "proxy password reset",
                |cfg| cfg.ss_password.clone().unwrap_or_default(),
            )
            .await
        }
    }
}

async fn password_status(args: &Args, ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<()> {
    let out = CtlOut::from_args(args, "proxy password status");
    require_ss_configured(&ctx.config)?;

    let db_override = get_setting(pool, settings_keys::SS_PASSWORD).await?;
    let configured = ctx
        .config
        .ss_password
        .as_ref()
        .is_some_and(|s| !s.is_empty());
    let source = if db_override.is_some() {
        "db"
    } else {
        "config"
    };

    if out.is_json() {
        return out.emit(json!({
            "configured": configured,
            "db_override": db_override.is_some(),
            "source": source,
        }));
    }

    out.blank();
    out.line(format!(
        "  Password source: {}",
        if db_override.is_some() {
            "DB override (value hidden)"
        } else {
            "config file (value hidden)"
        }
    ));
    out.blank();
    out.line(format!("  {RELOAD_HINT}"));
    out.blank();
    Ok(())
}

async fn setting_status(
    args: &Args,
    ctx: &CtlContext,
    pool: &chatmail_db::DbPool,
    key: &str,
    command: &'static str,
    label: &str,
    config_default: &str,
) -> Result<()> {
    let out = CtlOut::from_args(args, command);
    require_ss_configured(&ctx.config)?;

    let db_override = get_setting(pool, key).await?;
    let effective = db_override
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(config_default);
    let source = if db_override.is_some() {
        "db"
    } else {
        "config"
    };

    if out.is_json() {
        return out.emit(json!({
            label: effective,
            "db_override": db_override,
            "source": source,
        }));
    }

    out.blank();
    out.line(format!("  Effective {label}: {effective}"));
    match db_override {
        Some(v) => out.line(format!("  DB override:       {v}")),
        None => out.line("  DB override:       (none — using config file)"),
    }
    out.blank();
    out.line(format!("  {RELOAD_HINT}"));
    out.blank();
    Ok(())
}

async fn setting_set<F>(
    args: &Args,
    ctx: &CtlContext,
    pool: &chatmail_db::DbPool,
    key: &str,
    command: &'static str,
    value: &str,
    validate: F,
) -> Result<()>
where
    F: FnOnce(&str) -> Result<String>,
{
    require_ss_configured(&ctx.config)?;
    let out = CtlOut::from_args(args, command);
    let value = validate(value)?;
    set_setting(pool, key, &value).await?;
    out.done_msg(
        format!("✅ Proxy setting updated ({RELOAD_HINT})"),
        json!({ "value": value, "reload_required": true }),
        "Proxy setting updated",
    )
}

async fn setting_reset<F>(
    args: &Args,
    ctx: &CtlContext,
    pool: &chatmail_db::DbPool,
    key: &str,
    command: &'static str,
    config_default: F,
) -> Result<()>
where
    F: FnOnce(&chatmail_config::AppConfig) -> String,
{
    require_ss_configured(&ctx.config)?;
    let out = CtlOut::from_args(args, command);
    delete_setting(pool, key).await?;
    let effective = config_default(&ctx.config);
    out.done_msg(
        format!("🔄 Proxy DB override cleared (effective: {effective}; {RELOAD_HINT})"),
        json!({ "effective": effective, "source": "config", "reload_required": true }),
        "Proxy DB override cleared",
    )
}

async fn proxy_status_data(ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<Value> {
    if !ctx.config.ss_configured() {
        return Ok(json!({
            "configured": false,
            "enabled": false,
            "port": "",
            "cipher": "",
            "cipher_source": "config",
            "password_db_override": false,
            "shadowsocks_url": "",
            "ws_enabled": false,
            "grpc_enabled": false,
        }));
    }

    let mail_domain = mail_domain(ctx);
    let rt = resolve_runtime(pool, &ctx.config, &mail_domain, &ctx.state_dir).await?;
    let cipher_db = get_setting(pool, settings_keys::SS_CIPHER).await?;
    let password_db = get_setting(pool, settings_keys::SS_PASSWORD).await?;
    let (_, port) = rt.listen_addr.rsplit_once(':').unwrap_or(("", ""));
    let urls = rt.urls(&mail_domain);

    Ok(json!({
        "configured": true,
        "enabled": rt.enabled,
        "port": port,
        "cipher": rt.cipher,
        "cipher_source": if cipher_db.is_some() { "db" } else { "config" },
        "password_db_override": password_db.is_some(),
        "shadowsocks_url": urls.shadowsocks_url,
        "ws_enabled": rt.ws_enabled,
        "grpc_enabled": rt.grpc_enabled,
    }))
}

fn mail_domain(ctx: &CtlContext) -> String {
    ctx.config.effective_registration_domain(None)
}

fn require_ss_configured(config: &chatmail_config::AppConfig) -> Result<()> {
    if config.ss_configured() {
        Ok(())
    } else {
        Err(ChatmailError::config(SS_NOT_CONFIGURED))
    }
}

fn validate_cipher(cipher: &str) -> Result<String> {
    let trimmed = cipher.trim();
    if parse_cipher(trimmed).is_none() {
        return Err(ChatmailError::config(format!(
            "unsupported shadowsocks cipher: {cipher}\n\
             Supported: aes-128-gcm, aes-256-gcm, chacha20-ietf-poly1305"
        )));
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn validate_password(password: &str) -> Result<String> {
    let password = password.trim();
    if password.is_empty() {
        return Err(ChatmailError::config("password must not be empty"));
    }
    Ok(password.to_string())
}
