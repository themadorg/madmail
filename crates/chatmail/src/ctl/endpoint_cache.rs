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

//! `chatmail endpoint-cache` — Madmail `ctl/dnscache.go`.

use chatmail_config::cli::EndpointCacheCommand;
use chatmail_config::Args;
use chatmail_db::{
    get_endpoint_override, list_endpoint_overrides, remove_endpoint_override,
    set_endpoint_override,
};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;

pub async fn endpoint_cache(args: &Args, cmd: &EndpointCacheCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        EndpointCacheCommand::List => {
            let rows = list_endpoint_overrides(&pool).await?;
            if rows.is_empty() {
                eprintln!("No endpoint override entries.");
                return Ok(());
            }
            println!("LOOKUP KEY\tTARGET HOST\tCOMMENT\tCREATED AT\tUPDATED AT");
            for o in rows {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    o.lookup_key,
                    o.target_host,
                    o.comment.as_deref().unwrap_or(""),
                    o.created_at.as_deref().unwrap_or(""),
                    o.updated_at.as_deref().unwrap_or(""),
                );
            }
        }
        EndpointCacheCommand::Set {
            lookup_key,
            target_host,
            comment,
        } => {
            let comment = comment.as_deref().unwrap_or("");
            set_endpoint_override(&pool, lookup_key, target_host, comment).await?;
            println!("Successfully set endpoint override: {lookup_key} → {target_host}");
        }
        EndpointCacheCommand::Get { lookup_key } => {
            let row = get_endpoint_override(&pool, lookup_key)
                .await?
                .ok_or_else(|| {
                    ChatmailError::config(format!("no endpoint override found for {lookup_key:?}"))
                })?;
            println!("Lookup Key:\t{}", row.lookup_key);
            println!("Target Host:\t{}", row.target_host);
            println!("Comment:\t{}", row.comment.as_deref().unwrap_or(""));
            println!("Created At:\t{}", row.created_at.as_deref().unwrap_or(""));
            println!("Updated At:\t{}", row.updated_at.as_deref().unwrap_or(""));
        }
        EndpointCacheCommand::Remove { lookup_key } => {
            if !remove_endpoint_override(&pool, lookup_key).await? {
                return Err(ChatmailError::config(format!(
                    "no endpoint override found for {lookup_key:?}"
                )));
            }
            println!("Successfully removed endpoint override: {lookup_key}");
        }
    }
    Ok(())
}
