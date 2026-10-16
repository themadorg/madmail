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

//! `chatmail message-size` — `appendlimit` / `max_message_size` (`__APPENDLIMIT__`, `__MAX_MESSAGE_SIZE__`).

use chatmail_config::cli::MessageSizeCommand;
use chatmail_config::{format_data_size, parse_data_size, AppConfig, Args};
use chatmail_db::{delete_setting, get_setting, set_setting, settings_keys};
use chatmail_types::Result;

use super::context::CtlContext;
use super::output::CtlOut;

pub async fn message_size(args: &Args, cmd: Option<&MessageSizeCommand>) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        None | Some(MessageSizeCommand::Status) => status(args, &ctx, &pool).await,
        Some(MessageSizeCommand::Set { size }) => set_size(args, &ctx, &pool, size).await,
        Some(MessageSizeCommand::Reset) => reset(args, &pool, &ctx.config).await,
    }
}

async fn status(args: &Args, ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<()> {
    let out = CtlOut::from_args(args, "message-size status");
    let append = get_setting(pool, settings_keys::APPENDLIMIT).await?;
    let max = get_setting(pool, settings_keys::MAX_MESSAGE_SIZE).await?;
    let config_eff = chatmail_config::effective_max_message_bytes(&ctx.config);
    let effective =
        chatmail_config::resolve_max_message_bytes(config_eff, append.as_deref(), max.as_deref())?;

    if out.is_json() {
        let source = if append.is_some() || max.is_some() {
            "db"
        } else {
            "config"
        };
        return out.emit(serde_json::json!({
            "appendlimit": append,
            "max_message_size": max,
            "effective_bytes": effective,
            "source": source,
        }));
    }

    out.blank();
    out.line(format!(
        "  Effective limit:   {} ({} bytes)",
        format_data_size(effective),
        effective
    ));
    out.line(format!(
        "  Config file:       {} ({} bytes)",
        format_data_size(config_eff),
        config_eff
    ));
    match (&append, &max) {
        (Some(a), Some(b)) if a == b => {
            out.line(format!("  DB override:       {a}"));
        }
        (Some(a), Some(b)) => {
            out.line(format!("  DB appendlimit:    {a}"));
            out.line(format!("  DB max_message_size: {b}"));
        }
        (Some(a), None) => out.line(format!("  DB appendlimit:    {a}")),
        (None, Some(b)) => out.line(format!("  DB max_message_size: {b}")),
        (None, None) => out.line("  DB override:       (none — using config / default)"),
    }
    out.blank();
    out.line("  Apply to a running server: chatmail reload");
    out.blank();
    Ok(())
}

async fn set_size(
    args: &Args,
    ctx: &CtlContext,
    pool: &chatmail_db::DbPool,
    size: &str,
) -> Result<()> {
    let out = CtlOut::from_args(args, "message-size set");
    let size = size.trim();
    parse_data_size(size)?;
    pool.check_settings_writable(&[settings_keys::APPENDLIMIT, settings_keys::MAX_MESSAGE_SIZE])?;
    set_setting(pool, settings_keys::APPENDLIMIT, size).await?;
    set_setting(pool, settings_keys::MAX_MESSAGE_SIZE, size).await?;
    let config_eff = chatmail_config::effective_max_message_bytes(&ctx.config);
    let effective = chatmail_config::resolve_max_message_bytes(config_eff, Some(size), Some(size))?;
    out.done_msg(
        format!("📦 Message size limit set to {size} (effective {effective} bytes)"),
        serde_json::json!({
            "appendlimit": size,
            "max_message_size": size,
            "effective_bytes": effective,
        }),
        format!("Message size limit set to {size}"),
    )
}

async fn reset(args: &Args, pool: &chatmail_db::DbPool, config: &AppConfig) -> Result<()> {
    let out = CtlOut::from_args(args, "message-size reset");
    pool.check_settings_writable(&[settings_keys::APPENDLIMIT, settings_keys::MAX_MESSAGE_SIZE])?;
    delete_setting(pool, settings_keys::APPENDLIMIT).await?;
    delete_setting(pool, settings_keys::MAX_MESSAGE_SIZE).await?;
    let effective = chatmail_config::effective_max_message_bytes(config);
    out.done_msg(
        format!(
            "🔄 Message size DB overrides cleared (effective {} — {})",
            format_data_size(effective),
            effective
        ),
        serde_json::json!({ "effective_bytes": effective, "source": "config" }),
        "Message size DB overrides cleared",
    )
}
