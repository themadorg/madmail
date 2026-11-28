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
    create_sharing_contact, get_bool_setting, get_setting, init_sharing_db, list_sharing_contacts,
    remove_sharing_contact, set_setting, settings_keys, update_sharing_contact,
};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;
use super::output::CtlOut;

pub async fn sharing(args: &Args, cmd: &SharingCommand) -> Result<()> {
    match cmd {
        SharingCommand::Status => service_status(args).await,
        SharingCommand::Enable => service_set(args, true).await,
        SharingCommand::Disable => service_set(args, false).await,
        SharingCommand::List
        | SharingCommand::Create { .. }
        | SharingCommand::Reserve { .. }
        | SharingCommand::Remove { .. }
        | SharingCommand::Edit { .. } => links(args, cmd).await,
    }
}

async fn service_status(args: &Args) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let config_enabled = ctx.config.enable_contact_sharing;
    let stored = get_setting(&pool, settings_keys::CONTACT_SHARING_ENABLED).await?;
    let enabled = get_bool_setting(
        &pool,
        settings_keys::CONTACT_SHARING_ENABLED,
        config_enabled,
    )
    .await?;
    let source = if stored.is_some() {
        "database"
    } else {
        "config"
    };
    let out = CtlOut::from_args(args, "sharing status");
    let data = status_data(enabled, source, config_enabled);
    if out.is_json() {
        return out.emit(data);
    }
    out.blank();
    out.line(format!(
        "  Contact sharing: {}",
        if enabled { "enabled" } else { "disabled" }
    ));
    if stored.is_some() {
        out.line(format!(
            "  Source: database override (config enable_contact_sharing is {})",
            if config_enabled {
                "enabled"
            } else {
                "disabled"
            }
        ));
    } else {
        out.line("  Source: config enable_contact_sharing (no database override)");
    }
    if stored.is_some() {
        out.line("  Applies on the next HTTP request. No restart required.");
    } else {
        out.line(
            "  The running server keeps the file value from when it started. Editing the file needs a restart.",
        );
    }
    out.blank();
    Ok(())
}

async fn service_set(args: &Args, on: bool) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let config_enabled = ctx.config.enable_contact_sharing;
    set_setting(
        &pool,
        settings_keys::CONTACT_SHARING_ENABLED,
        if on { "true" } else { "false" },
    )
    .await?;
    let command = if on {
        "sharing enable"
    } else {
        "sharing disable"
    };
    let out = CtlOut::from_args(args, command);
    let (human, message) = if on {
        (
            "✅ Contact sharing enabled (effective on the next HTTP request)",
            "Contact sharing enabled",
        )
    } else {
        (
            "🚫 Contact sharing disabled (public /share and contact pages return 404; saved links are kept)",
            "Contact sharing disabled",
        )
    };
    out.done_msg(human, status_data(on, "database", config_enabled), message)
}

fn status_data(enabled: bool, source: &'static str, config_enabled: bool) -> serde_json::Value {
    serde_json::json!({
        "enabled": enabled,
        "source": source,
        "config_enabled": config_enabled,
        "reload_required": false,
        "file_change_requires_restart": source == "config",
    })
}

async fn links(args: &Args, cmd: &SharingCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let sharing_path = ctx.state_dir.join("sharing.db");
    let pool = init_sharing_db(&sharing_path).await?;
    let out = CtlOut::from_args(args, "sharing");

    match cmd {
        SharingCommand::List => {
            let contacts = list_sharing_contacts(&pool).await?;
            if out.is_json() {
                let entries: Vec<_> = contacts
                    .into_iter()
                    .map(|c| {
                        serde_json::json!({
                            "slug": c.slug,
                            "name": c.name,
                            "url": c.url,
                            "created_at": c.created_at,
                        })
                    })
                    .collect();
                return out.emit(serde_json::json!({ "entries": entries }));
            }
            out.line("SLUG\tNAME\tURL\tCREATED AT");
            for c in contacts {
                out.line(format!(
                    "{}\t{}\t{}\t{}",
                    c.slug, c.name, c.url, c.created_at
                ));
            }
        }
        SharingCommand::Create { slug, url, name } => {
            let name = name.as_deref().unwrap_or("");
            create_sharing_contact(&pool, slug, url, name).await?;
            out.done_msg(
                format!("Successfully created link: {slug}"),
                serde_json::json!({ "slug": slug, "url": url, "name": name }),
                format!("Created link: {slug}"),
            )?;
        }
        SharingCommand::Reserve { slug } => {
            create_sharing_contact(&pool, slug, "reserved", "Reserved").await?;
            out.done_msg(
                format!("Successfully created link: {slug}"),
                serde_json::json!({ "slug": slug, "reserved": true }),
                format!("Reserved slug: {slug}"),
            )?;
        }
        SharingCommand::Remove { slug } => {
            if !remove_sharing_contact(&pool, slug).await? {
                return Err(ChatmailError::config(format!("slug {slug} not found")));
            }
            out.done_msg(
                format!("Successfully removed link: {slug}"),
                serde_json::json!({ "slug": slug }),
                format!("Removed link: {slug}"),
            )?;
        }
        SharingCommand::Edit { slug, url, name } => {
            if !update_sharing_contact(&pool, slug, url, name.as_deref()).await? {
                return Err(ChatmailError::config(format!("slug {slug} not found")));
            }
            out.done_msg(
                format!("Successfully updated link: {slug}"),
                serde_json::json!({ "slug": slug, "url": url, "name": name }),
                format!("Updated link: {slug}"),
            )?;
        }
        SharingCommand::Status | SharingCommand::Enable | SharingCommand::Disable => {
            unreachable!("service switch is handled before link commands");
        }
    }
    Ok(())
}
