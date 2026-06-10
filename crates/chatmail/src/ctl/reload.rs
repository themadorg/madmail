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

//! `chatmail reload` — Madmail `ctl/reload_config.go` (POST `/admin/reload`).

use chatmail_config::Args;
use chatmail_state::ReloadScope;
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;
use super::request_reload::{request_soft_reload, SoftReloadOutcome};

pub async fn reload(args: &Args, url_override: Option<&str>, insecure: bool) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    ctx.require_db()?;

    match request_soft_reload(&ctx, url_override, insecure, ReloadScope::Full, false).await? {
        SoftReloadOutcome::Accepted { api_url } => {
            println!(
                "✅ Soft reload requested at {api_url} — listeners and HTTP routes restart in place (no process exit)."
            );
        }
        SoftReloadOutcome::ServerNotRunning => {
            return Err(ChatmailError::config(
                "could not reach admin API — is the server running? Use --url or check hostname/ports",
            ));
        }
    }
    Ok(())
}
