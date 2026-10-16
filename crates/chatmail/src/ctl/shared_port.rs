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

//! `madmail shared-port` — mail on the HTTPS port
//! (`__SHARED_PORT_IMAP__` / `__SHARED_PORT_SMTP__`).

use chatmail_config::cli::SharedPortCommand;
use chatmail_config::{parse_bool_str, Args};
use chatmail_db::{get_setting, set_setting, settings_keys, DbPool};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;
use super::output::CtlOut;

/// Which protocols a subcommand's `PROTOCOL` argument names.
fn selected(protocol: &str) -> Result<(bool, bool)> {
    match protocol.trim().to_ascii_lowercase().as_str() {
        "both" | "all" => Ok((true, true)),
        "imap" => Ok((true, false)),
        "smtp" | "submission" => Ok((false, true)),
        other => Err(ChatmailError::config(format!(
            "unknown protocol: {other} (expected imap, smtp, or both)"
        ))),
    }
}

/// Effective value, and whether it came from the DB override or the config file.
async fn effective(pool: &DbPool, key: &str, file_default: bool) -> Result<(bool, Option<bool>)> {
    Ok(match get_setting(pool, key).await? {
        Some(v) => {
            let on = parse_bool_str(&v);
            (on, Some(on))
        }
        None => (file_default, None),
    })
}

pub async fn shared_port(args: &Args, cmd: &SharedPortCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let file_imap = ctx.config.alpn_imap.is_some();
    let file_smtp = ctx.config.alpn_smtp.is_some();

    match cmd {
        SharedPortCommand::Status => {
            let out = CtlOut::from_args(args, "shared-port status");
            let (imap, imap_db) =
                effective(&pool, settings_keys::SHARED_PORT_IMAP, file_imap).await?;
            let (smtp, smtp_db) =
                effective(&pool, settings_keys::SHARED_PORT_SMTP, file_smtp).await?;
            // Without the block there is no demux to toggle, whatever the DB says.
            let configured = file_imap || file_smtp;
            if out.is_json() {
                return out.emit(serde_json::json!({
                    "imap": imap,
                    "smtp": smtp,
                    "file_default": { "imap": file_imap, "smtp": file_smtp },
                    "db_override": { "imap": imap_db, "smtp": smtp_db },
                    "chatmail_block_configured": configured,
                    "reload_required": false,
                }));
            }
            out.blank();
            out.line(format!(
                "  IMAP on the HTTPS port:       {}",
                if imap { "served" } else { "not served" }
            ));
            out.line(format!(
                "  Submission on the HTTPS port: {}",
                if smtp { "served" } else { "not served" }
            ));
            out.line(format!(
                "  File default (alpn_imap / alpn_smtp): {} / {}",
                file_imap, file_smtp
            ));
            for (label, db) in [
                ("__SHARED_PORT_IMAP__", imap_db),
                ("__SHARED_PORT_SMTP__", smtp_db),
            ] {
                match db {
                    Some(on) => out.line(format!("  DB override ({label}): {on}")),
                    None => out.line(format!(
                        "  DB override ({label}): (unset — using file default)"
                    )),
                }
            }
            if !configured {
                out.line("  ⚠️  No `chatmail tls://…` block in the config: the HTTPS port is a");
                out.line("      plain listener, so these settings do nothing until one is added.");
            }
            out.line("  Apply on a running server: madmail reload");
            out.blank();
            Ok(())
        }
        SharedPortCommand::Enable { protocol } | SharedPortCommand::Disable { protocol } => {
            let on = matches!(cmd, SharedPortCommand::Enable { .. });
            let (want_imap, want_smtp) = selected(protocol)?;

            let out = CtlOut::from_args(
                args,
                if on {
                    "shared-port enable"
                } else {
                    "shared-port disable"
                },
            );
            let mut keys = Vec::new();
            if want_imap {
                keys.push(settings_keys::SHARED_PORT_IMAP);
            }
            if want_smtp {
                keys.push(settings_keys::SHARED_PORT_SMTP);
            }
            pool.check_settings_writable(&keys)?;
            let value = if on { "true" } else { "false" };
            if want_imap {
                set_setting(&pool, settings_keys::SHARED_PORT_IMAP, value).await?;
            }
            if want_smtp {
                set_setting(&pool, settings_keys::SHARED_PORT_SMTP, value).await?;
            }
            let state = if on { "served" } else { "not served" };
            out.done_msg(
                format!(
                    "HTTPS port: {}{}{} {state}. Run: madmail reload",
                    if want_imap { "IMAP" } else { "" },
                    if want_imap && want_smtp { " and " } else { "" },
                    if want_smtp { "submission" } else { "" },
                ),
                serde_json::json!({
                    "imap": want_imap.then_some(on),
                    "smtp": want_smtp.then_some(on),
                    "reload_required": true,
                }),
                if on {
                    "shared port enabled"
                } else {
                    "shared port disabled"
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P12-UT23: the protocol argument is the only free-text input here, so a typo
    /// must be an error rather than a silent no-op on the wrong protocol.
    #[test]
    fn protocol_argument_selects_protocols() {
        assert_eq!(selected("both").unwrap(), (true, true));
        assert_eq!(selected("IMAP").unwrap(), (true, false));
        assert_eq!(selected(" smtp ").unwrap(), (false, true));
        assert_eq!(selected("submission").unwrap(), (false, true));
        assert!(selected("imap4").is_err());
        assert!(selected("").is_err());
    }
}
