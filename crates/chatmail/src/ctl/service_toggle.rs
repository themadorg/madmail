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

pub async fn run(
    args: &Args,
    setting_key: &str,
    service_label: &str,
    cmd: &ServiceToggleCommand,
) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        ServiceToggleCommand::Status => status(&pool, setting_key, service_label).await,
        ServiceToggleCommand::Enable => set_flag(&pool, setting_key, service_label, true).await,
        ServiceToggleCommand::Disable => set_flag(&pool, setting_key, service_label, false).await,
    }
}

async fn status(pool: &DbPool, key: &str, label: &str) -> Result<()> {
    let on = get_bool_setting(pool, key, false).await?;
    println!();
    println!("  {label}: {}", if on { "enabled" } else { "disabled" });
    println!("  (effective on next HTTP request; no restart required)");
    println!();
    Ok(())
}

async fn set_flag(pool: &DbPool, key: &str, label: &str, on: bool) -> Result<()> {
    set_setting(pool, key, if on { "true" } else { "false" }).await?;
    if on {
        println!("✅ {label} enabled (effective immediately on next HTTP request)");
    } else {
        println!("🚫 {label} disabled (API returns 404 when disabled)");
    }
    Ok(())
}
