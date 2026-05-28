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

//! `chatmail sharing` — Madmail `ctl/sharing.go`.

use chatmail_config::cli::SharingCommand;
use chatmail_config::Args;
use chatmail_db::{
    create_sharing_contact, init_sharing_db, list_sharing_contacts, remove_sharing_contact,
    update_sharing_contact,
};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;

pub async fn sharing(args: &Args, cmd: &SharingCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let sharing_path = ctx.state_dir.join("sharing.db");
    let pool = init_sharing_db(&sharing_path).await?;

    match cmd {
        SharingCommand::List => {
            let contacts = list_sharing_contacts(&pool).await?;
            if contacts.is_empty() {
                println!("SLUG\tNAME\tURL\tCREATED AT");
                return Ok(());
            }
            println!("SLUG\tNAME\tURL\tCREATED AT");
            for c in contacts {
                println!("{}\t{}\t{}\t{}", c.slug, c.name, c.url, c.created_at);
            }
        }
        SharingCommand::Create { slug, url, name } => {
            let name = name.as_deref().unwrap_or("");
            create_sharing_contact(&pool, slug, url, name).await?;
            println!("Successfully created link: {slug}");
        }
        SharingCommand::Reserve { slug } => {
            create_sharing_contact(&pool, slug, "reserved", "Reserved").await?;
            println!("Successfully created link: {slug}");
        }
        SharingCommand::Remove { slug } => {
            if !remove_sharing_contact(&pool, slug).await? {
                return Err(ChatmailError::config(format!("slug {slug} not found")));
            }
            println!("Successfully removed link: {slug}");
        }
        SharingCommand::Edit { slug, url, name } => {
            if !update_sharing_contact(&pool, slug, url, name.as_deref()).await? {
                return Err(ChatmailError::config(format!("slug {slug} not found")));
            }
            println!("Successfully updated link: {slug}");
        }
    }
    Ok(())
}
