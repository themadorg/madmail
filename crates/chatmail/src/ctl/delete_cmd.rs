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

//! `chatmail delete` — full account removal + blocklist.

use chatmail_auth::normalize_username;
use chatmail_config::Args;
use chatmail_db::CLI_DELETE_REASON;
use chatmail_storage::MailboxStore;
use chatmail_types::Result;

use super::account_ops::delete_account_full;
use super::context::CtlContext;
use super::util::confirm;

pub async fn delete(
    args: &Args,
    username: &str,
    yes: bool,
    reason: &str,
) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let mailbox = MailboxStore::new(&ctx.state_dir);

    let host = ctx
        .config
        .hostname
        .as_deref()
        .unwrap_or("127.0.0.1");
    let domain = ctx.config.effective_registration_domain(Some(host));
    let u = if username.trim().contains('@') {
        normalize_username(username.trim())?
    } else {
        normalize_username(&format!("{}@{domain}", username.trim()))?
    };

    let reason = if reason.is_empty() {
        CLI_DELETE_REASON
    } else {
        reason
    };

    if !confirm(
        &format!("Delete account {u} (credentials, mail, blocklist)?"),
        yes,
    )? {
        println!("Aborted.");
        return Ok(());
    }

    delete_account_full(&pool, &mailbox, &u, reason).await?;
    println!("Deleted and blocklisted: {u}");
    println!("Reason: {reason}");
    Ok(())
}
