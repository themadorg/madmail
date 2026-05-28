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
use chatmail_config::{format_data_size, parse_data_size, Args, AppConfig};
use chatmail_db::{delete_setting, get_setting, set_setting, settings_keys};
use chatmail_types::Result;

use super::context::CtlContext;

pub async fn message_size(args: &Args, cmd: Option<&MessageSizeCommand>) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        None | Some(MessageSizeCommand::Status) => status(&ctx, &pool).await,
        Some(MessageSizeCommand::Set { size }) => set_size(&ctx, &pool, size).await,
        Some(MessageSizeCommand::Reset) => reset(&pool, &ctx.config).await,
    }
}

async fn status(ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<()> {
    let append = get_setting(pool, settings_keys::APPENDLIMIT).await?;
    let max = get_setting(pool, settings_keys::MAX_MESSAGE_SIZE).await?;
    let config_eff = chatmail_config::effective_max_message_bytes(&ctx.config);
    let effective = chatmail_config::resolve_max_message_bytes(
        config_eff,
        append.as_deref(),
        max.as_deref(),
    )?;

    println!();
    println!(
        "  Effective limit:   {} ({} bytes)",
        format_data_size(effective),
        effective
    );
    println!(
        "  Config file:       {} ({} bytes)",
        format_data_size(config_eff),
        config_eff
    );
    match (&append, &max) {
        (Some(a), Some(b)) if a == b => {
            println!("  DB override:       {a}");
        }
        (Some(a), Some(b)) => {
            println!("  DB appendlimit:    {a}");
            println!("  DB max_message_size: {b}");
        }
        (Some(a), None) => println!("  DB appendlimit:    {a}"),
        (None, Some(b)) => println!("  DB max_message_size: {b}"),
        (None, None) => println!("  DB override:       (none — using config / default)"),
    }
    println!();
    println!("  Apply to a running server: chatmail reload");
    println!();
    Ok(())
}

async fn set_size(ctx: &CtlContext, pool: &chatmail_db::DbPool, size: &str) -> Result<()> {
    let size = size.trim();
    parse_data_size(size)?;
    set_setting(pool, settings_keys::APPENDLIMIT, size).await?;
    set_setting(pool, settings_keys::MAX_MESSAGE_SIZE, size).await?;
    let config_eff = chatmail_config::effective_max_message_bytes(&ctx.config);
    let effective =
        chatmail_config::resolve_max_message_bytes(config_eff, Some(size), Some(size))?;
    println!(
        "📦 Message size limit set to {size} (effective {} bytes)",
        effective
    );
    println!("   Run `chatmail reload` if the server is already running.");
    Ok(())
}

async fn reset(pool: &chatmail_db::DbPool, config: &AppConfig) -> Result<()> {
    delete_setting(pool, settings_keys::APPENDLIMIT).await?;
    delete_setting(pool, settings_keys::MAX_MESSAGE_SIZE).await?;
    let effective = chatmail_config::effective_max_message_bytes(config);
    println!(
        "🔄 Message size DB overrides cleared (effective {} — {})",
        format_data_size(effective),
        effective
    );
    println!("   Run `chatmail reload` if the server is already running.");
    Ok(())
}
