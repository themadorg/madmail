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

//! `chatmail registration` — open / close / status (`__REGISTRATION_OPEN__`).

use chatmail_config::cli::RegistrationCommand;
use chatmail_config::Args;
use chatmail_db::{get_bool_setting, set_setting, settings_keys};
use chatmail_types::Result;

use super::context::CtlContext;

pub async fn registration(args: &Args, cmd: &RegistrationCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        RegistrationCommand::Open => {
            set_setting(&pool, settings_keys::REGISTRATION_OPEN, "true").await?;
            println!("Registration is now OPEN");
        }
        RegistrationCommand::Close => {
            set_setting(&pool, settings_keys::REGISTRATION_OPEN, "false").await?;
            println!("Registration is now CLOSED");
        }
        RegistrationCommand::Status => {
            let open = get_bool_setting(&pool, settings_keys::REGISTRATION_OPEN, false).await?;
            if open {
                println!("Registration is OPEN");
            } else {
                println!("Registration is CLOSED");
            }
        }
    }
    Ok(())
}
