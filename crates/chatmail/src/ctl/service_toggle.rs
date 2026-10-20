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

//! Enable / disable / status for DB-backed service toggles (`__*__ENABLED__` keys).

use chatmail_config::cli::ServiceToggleCommand;
use chatmail_config::Args;
use chatmail_db::{get_bool_setting, set_setting, DbPool};
use chatmail_types::Result;

use super::context::CtlContext;
use super::output::CtlOut;

pub async fn run(
    args: &Args,
    command: &'static str,
    default: fn(&chatmail_config::AppConfig) -> bool,
    setting_key: &str,
    service_label: &str,
    cmd: &ServiceToggleCommand,
) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        ServiceToggleCommand::Status => {
            status(
                args,
                command,
                &pool,
                setting_key,
                service_label,
                default(&ctx.config),
            )
            .await
        }
        ServiceToggleCommand::Enable => {
            set_flag(args, command, &pool, setting_key, service_label, true).await
        }
        ServiceToggleCommand::Disable => {
            set_flag(args, command, &pool, setting_key, service_label, false).await
        }
    }
}

async fn status(
    args: &Args,
    command: &'static str,
    pool: &DbPool,
    key: &str,
    label: &str,
    default: bool,
) -> Result<()> {
    let out = CtlOut::from_args(args, command);
    let on = get_bool_setting(pool, key, default).await?;
    if out.is_json() {
        return out.emit(serde_json::json!({ "enabled": on }));
    }
    out.blank();
    out.line(format!(
        "  {label}: {}",
        if on { "enabled" } else { "disabled" }
    ));
    out.line("  (effective on next HTTP request; no restart required)");
    out.blank();
    Ok(())
}

async fn set_flag(
    args: &Args,
    command: &'static str,
    pool: &DbPool,
    key: &str,
    label: &str,
    on: bool,
) -> Result<()> {
    let out = CtlOut::from_args(args, command);
    set_setting(pool, key, if on { "true" } else { "false" }).await?;
    let msg = if on {
        format!("{label} enabled")
    } else {
        format!("{label} disabled")
    };
    out.done_msg(
        if on {
            format!("✅ {label} enabled (effective immediately on next HTTP request)")
        } else {
            format!("🚫 {label} disabled (API returns 404 when disabled)")
        },
        serde_json::json!({ "enabled": on }),
        msg,
    )
}
