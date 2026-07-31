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

//! `madmail openrelay` — inbound open-relay-class RCPT (`__ALLOW_INBOUND_REMOTE_RCPT__`).

use chatmail_config::cli::OpenrelayCommand;
use chatmail_config::{parse_bool_str, Args};
use chatmail_db::{get_setting, set_setting, settings_keys};
use chatmail_types::Result;

use super::context::CtlContext;
use super::output::CtlOut;

pub async fn openrelay(args: &Args, cmd: &OpenrelayCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        OpenrelayCommand::Status => {
            let out = CtlOut::from_args(args, "openrelay status");
            let file_default = ctx.config.allow_inbound_remote_rcpt;
            let (db_override, allowed) =
                match get_setting(&pool, settings_keys::ALLOW_INBOUND_REMOTE_RCPT).await? {
                    Some(v) => {
                        let on = parse_bool_str(&v);
                        (Some(on), on)
                    }
                    None => (None, file_default),
                };
            if out.is_json() {
                return out.emit(serde_json::json!({
                    "allowed": allowed,
                    "file_default": file_default,
                    "db_override": db_override,
                    "reload_required": false,
                }));
            }
            out.blank();
            out.line(format!(
                "  Inbound remote RCPT (open relay): {}",
                if allowed { "allowed" } else { "denied" }
            ));
            out.line(format!(
                "  File default (allow_inbound_remote_rcpt): {}",
                if file_default { "true" } else { "false" }
            ));
            match db_override {
                Some(on) => out.line(format!(
                    "  DB override (__ALLOW_INBOUND_REMOTE_RCPT__): {}",
                    if on { "true" } else { "false" }
                )),
                None => out.line("  DB override: (unset — using file default)"),
            }
            out.line("  Authenticated submission (587/465) always allows remote RCPT.");
            out.line("  Apply DB changes on a running server: madmail reload");
            out.blank();
            Ok(())
        }
        OpenrelayCommand::Enable => {
            let out = CtlOut::from_args(args, "openrelay enable");
            set_setting(&pool, settings_keys::ALLOW_INBOUND_REMOTE_RCPT, "true").await?;
            out.done_msg(
                "⚠️  Inbound remote RCPT ENABLED (open-relay-class). Run: madmail reload",
                serde_json::json!({
                    "allowed": true,
                    "reload_required": true,
                }),
                "Inbound remote RCPT enabled",
            )
        }
        OpenrelayCommand::Disable => {
            let out = CtlOut::from_args(args, "openrelay disable");
            set_setting(&pool, settings_keys::ALLOW_INBOUND_REMOTE_RCPT, "false").await?;
            out.done_msg(
                "Inbound remote RCPT DENIED (default-safe). Run: madmail reload",
                serde_json::json!({
                    "allowed": false,
                    "reload_required": true,
                }),
                "Inbound remote RCPT disabled",
            )
        }
    }
}
