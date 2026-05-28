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

//! `chatmail blocklist` and shared ban-list output.

use chatmail_config::cli::BlocklistCommand;
use chatmail_config::Args;
use chatmail_db::{blocklist, DbPool, MANUAL_BLOCK_REASON};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;
use super::util::confirm;

pub async fn blocklist(args: &Args, cmd: &BlocklistCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        BlocklistCommand::List => print_ban_list(&pool).await,
        BlocklistCommand::Add { username, reason } => {
            let u = normalize_blocklist_username(username)?;
            let r = reason.as_deref().unwrap_or(MANUAL_BLOCK_REASON);
            blocklist::block_user(&pool, &u, r).await?;
            println!("Blocked: {u} ({r})");
            Ok(())
        }
        BlocklistCommand::Remove { username, yes } => {
            let u = normalize_blocklist_username(username)?;
            if !confirm(&format!("Unblock {u}?"), *yes)? {
                println!("Aborted.");
                return Ok(());
            }
            blocklist::unblock_user(&pool, &u).await?;
            println!("Unblocked: {u}");
            Ok(())
        }
    }
}

pub async fn print_ban_list(pool: &DbPool) -> Result<()> {
    let rows = blocklist::list_blocked_users(pool).await?;
    if rows.is_empty() {
        println!("(no blocked users)");
        return Ok(());
    }
    for (username, reason, blocked_at) in rows {
        println!("{username}\t{reason}\t{blocked_at}");
    }
    Ok(())
}

fn normalize_blocklist_username(raw: &str) -> Result<String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(ChatmailError::config("username is required"));
    }
    if t.contains('@') {
        chatmail_auth::normalize_username(t)
    } else {
        Ok(t.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_email_usernames() {
        assert_eq!(
            normalize_blocklist_username("User@1.2.3.4").unwrap(),
            "user@[1.2.3.4]"
        );
    }

    #[test]
    fn normalize_bare_localpart() {
        assert_eq!(normalize_blocklist_username("BadActor").unwrap(), "badactor");
    }

    #[tokio::test]
    async fn print_ban_list_empty_and_populated() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        print_ban_list(&pool).await.unwrap();

        blocklist::block_user(&pool, "a@x.org", "r1").await.unwrap();
        blocklist::block_user(&pool, "b@x.org", "r2").await.unwrap();
        // does not panic; output goes to stdout in real CLI
        print_ban_list(&pool).await.unwrap();
    }
}
