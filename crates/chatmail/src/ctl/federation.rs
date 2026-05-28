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

//! `chatmail federation` — Madmail `ctl/federation.go`.

use chatmail_config::cli::FederationCommand;
use chatmail_config::Args;
use chatmail_db::{federation_policy_label, set_federation_policy_label};
use chatmail_state::{FederationPolicyCache, FederationSilentDismissCache, FederationTracker};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;

pub async fn federation(args: &Args, cmd: &FederationCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let cache = FederationPolicyCache::new();
    cache.hydrate(&pool).await?;
    let dismiss = FederationSilentDismissCache::new();
    dismiss.hydrate(&pool).await?;

    match cmd {
        FederationCommand::Policy { policy } => {
            let p = policy.trim().to_ascii_uppercase();
            if p != "ACCEPT" && p != "REJECT" {
                return Err(ChatmailError::config("policy must be 'accept' or 'reject'"));
            }
            set_federation_policy_label(&pool, &p).await?;
            println!("Success: Global federation policy switched to {p}.");
        }
        FederationCommand::Block { domain } => {
            if domain.trim().is_empty() {
                return Err(ChatmailError::config("DOMAIN is required"));
            }
            let count = cache.add_rule_count(&pool, domain).await?;
            println!(
                "Success: '{domain}' added to rules. Currently blocking {count} total domain(s)."
            );
        }
        FederationCommand::Allow { domain } => {
            if domain.trim().is_empty() {
                return Err(ChatmailError::config("DOMAIN is required"));
            }
            let count = cache.add_rule_count(&pool, domain).await?;
            println!(
                "Success: '{domain}' added to rules. Currently trusting {count} total domain(s)."
            );
        }
        FederationCommand::Remove { domain } => {
            if domain.trim().is_empty() {
                return Err(ChatmailError::config("DOMAIN is required"));
            }
            let remaining = cache.remove_rule_count(&pool, domain).await?;
            println!("Success: Removed '{domain}' from rules. {remaining} remaining.");
        }
        FederationCommand::Flush => {
            cache.flush_rules(&pool).await?;
            println!("WARNING: Configuration flushed. 0 custom domains remain in active list.");
        }
        FederationCommand::List => {
            let policy = federation_policy_label(&pool).await?;
            println!("[ FEDERATION STATE ]");
            println!("Policy:   {policy}\n");
            let rules = cache.list_rules(&pool).await?;
            if rules.is_empty() {
                println!("[ ACTIVE RULES ]");
                println!("No exceptions configured.");
                println!("---");
                println!("Total: 0 exceptions.");
                return Ok(());
            }
            println!("[ ACTIVE RULES ]");
            for (i, (domain, created_at)) in rules.iter().enumerate() {
                let date = format_created_at(*created_at);
                println!("{}. {domain}\t(Added: {date})", i + 1);
            }
            println!("---");
            println!("Total: {} exceptions.", rules.len());
        }
        FederationCommand::Dismiss { domain } => {
            if domain.trim().is_empty() {
                return Err(ChatmailError::config("DOMAIN is required"));
            }
            let count = dismiss.add_count(&pool, domain).await?;
            println!(
                "Success: '{domain}' added to silent dismiss. Currently dismissing {count} domain(s)."
            );
        }
        FederationCommand::Undismiss { domain } => {
            if domain.trim().is_empty() {
                return Err(ChatmailError::config("DOMAIN is required"));
            }
            let remaining = dismiss.remove_count(&pool, domain).await?;
            println!(
                "Success: Removed '{domain}' from silent dismiss. {remaining} remaining."
            );
        }
        FederationCommand::DismissList => {
            let rules = dismiss.list_rules(&pool).await?;
            println!("[ SILENT DISMISS ]");
            if rules.is_empty() {
                println!("No domains configured.");
                println!("---");
                println!("Total: 0 domains.");
                return Ok(());
            }
            for (i, (domain, created_at)) in rules.iter().enumerate() {
                let date = format_created_at(*created_at);
                println!("{}. {domain}\t(Added: {date})", i + 1);
            }
            println!("---");
            println!("Total: {} domains.", rules.len());
        }
        FederationCommand::DismissFlush => {
            dismiss.flush(&pool).await?;
            println!("WARNING: Silent dismiss list flushed. 0 domains remain.");
        }
        FederationCommand::Status => {
            let tracker = FederationTracker::new();
            tracker.hydrate(&pool).await?;
            let stats = tracker.snapshot();
            if stats.is_empty() {
                println!("[ TRAFFIC ANOMALIES ]");
                println!("No federation traffic recorded.");
                return Ok(());
            }
            println!("[ TRAFFIC ANOMALIES ]");
            for s in stats {
                let total_failed = s.failed_http + s.failed_https + s.failed_smtp;
                let mut success_info = format!("{} Delivered", s.successful_deliveries);
                let mut parts = Vec::new();
                if s.success_https > 0 {
                    parts.push(format!("{} HTTPS", s.success_https));
                }
                if s.success_http > 0 {
                    parts.push(format!("{} HTTP", s.success_http));
                }
                if s.success_smtp > 0 {
                    parts.push(format!("{} SMTP", s.success_smtp));
                }
                if !parts.is_empty() {
                    success_info.push_str(&format!(" ({})", parts.join(", ")));
                }
                let mut fail_info = String::new();
                if s.failed_https > 0 {
                    fail_info.push_str(&format!(" {} Failed (HTTPS)", s.failed_https));
                }
                if s.failed_http > 0 {
                    fail_info.push_str(&format!(" {} Failed (HTTP)", s.failed_http));
                }
                if s.failed_smtp > 0 {
                    fail_info.push_str(&format!(" {} Failed (SMTP)", s.failed_smtp));
                }
                if fail_info.is_empty() && total_failed == 0 {
                    fail_info = " 0 Failed".into();
                }
                println!(
                    "- {} : {} / {} pending /{}",
                    s.domain, success_info, s.queued_messages, fail_info
                );
            }
        }
    }
    Ok(())
}

fn format_created_at(ts: i64) -> String {
    let Ok(fmt) = time::format_description::parse("[year]-[month]-[day]") else {
        return "unknown".into();
    };
    time::OffsetDateTime::from_unix_timestamp(ts)
        .ok()
        .and_then(|dt| dt.format(&fmt).ok())
        .unwrap_or_else(|| "unknown".into())
}
