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

use chatmail_config::cli::{FederationCommand, FederationTransportCommand};
use chatmail_config::Args;
use chatmail_db::{federation_policy_label, set_federation_policy_label, DbPool};
use chatmail_state::{
    load_transport_policy, parse_transport_mode, set_transport_mode, FederationPolicyCache,
    FederationSilentDismissCache, FederationTracker, TransportName,
};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;
use super::output::CtlOut;

pub async fn federation(args: &Args, cmd: &FederationCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let cache = FederationPolicyCache::new();
    cache.hydrate(&pool).await?;
    let dismiss = FederationSilentDismissCache::new();
    dismiss.hydrate(&pool).await?;
    let out = CtlOut::from_args(args, "federation");

    match cmd {
        FederationCommand::Policy { policy } => {
            let p = policy.trim().to_ascii_uppercase();
            if p != "ACCEPT" && p != "REJECT" {
                return Err(ChatmailError::config("policy must be 'accept' or 'reject'"));
            }
            set_federation_policy_label(&pool, &p).await?;
            return out.done_msg(
                format!("Success: Global federation policy switched to {p}."),
                serde_json::json!({ "policy": p }),
                format!("Federation policy set to {p}"),
            );
        }
        FederationCommand::Block { domain } => {
            if domain.trim().is_empty() {
                return Err(ChatmailError::config("DOMAIN is required"));
            }
            let count = cache.add_rule_count(&pool, domain).await?;
            return out.done_msg(
                format!(
                    "Success: '{domain}' added to rules. Currently blocking {count} total domain(s)."
                ),
                serde_json::json!({ "domain": domain, "action": "block", "total": count }),
                format!("Added {domain} to federation rules"),
            );
        }
        FederationCommand::Allow { domain } => {
            if domain.trim().is_empty() {
                return Err(ChatmailError::config("DOMAIN is required"));
            }
            let count = cache.add_rule_count(&pool, domain).await?;
            return out.done_msg(
                format!(
                    "Success: '{domain}' added to rules. Currently trusting {count} total domain(s)."
                ),
                serde_json::json!({ "domain": domain, "action": "allow", "total": count }),
                format!("Added {domain} to federation rules"),
            );
        }
        FederationCommand::Remove { domain } => {
            if domain.trim().is_empty() {
                return Err(ChatmailError::config("DOMAIN is required"));
            }
            let remaining = cache.remove_rule_count(&pool, domain).await?;
            return out.done_msg(
                format!("Success: Removed '{domain}' from rules. {remaining} remaining."),
                serde_json::json!({ "domain": domain, "remaining": remaining }),
                format!("Removed {domain} from federation rules"),
            );
        }
        FederationCommand::Flush => {
            cache.flush_rules(&pool).await?;
            return out.done_msg(
                "WARNING: Configuration flushed. 0 custom domains remain in active list.",
                serde_json::json!({ "flushed": true, "total": 0 }),
                "Federation rules flushed",
            );
        }
        FederationCommand::List => {
            let policy = federation_policy_label(&pool).await?;
            let rules = cache.list_rules(&pool).await?;
            let action = rule_action(&policy);
            if out.is_json() {
                let rules_json: Vec<_> = rules
                    .into_iter()
                    .map(|(domain, _)| serde_json::json!({ "domain": domain, "action": action }))
                    .collect();
                return out.emit(serde_json::json!({
                    "policy": policy,
                    "rules": rules_json,
                }));
            }
            out.line("[ FEDERATION STATE ]");
            out.line(format!("Policy:   {policy}\n"));
            if rules.is_empty() {
                out.line("[ ACTIVE RULES ]");
                out.line("No exceptions configured.");
                out.line("---");
                out.line("Total: 0 exceptions.");
                return Ok(());
            }
            out.line("[ ACTIVE RULES ]");
            for (i, (domain, created_at)) in rules.iter().enumerate() {
                let date = format_created_at(*created_at);
                out.line(format!("{}. {domain}\t(Added: {date})", i + 1));
            }
            out.line("---");
            out.line(format!("Total: {} exceptions.", rules.len()));
        }
        FederationCommand::Dismiss { domain } => {
            if domain.trim().is_empty() {
                return Err(ChatmailError::config("DOMAIN is required"));
            }
            let count = dismiss.add_count(&pool, domain).await?;
            return out.done_msg(
                format!(
                    "Success: '{domain}' added to silent dismiss. Currently dismissing {count} domain(s)."
                ),
                serde_json::json!({ "domain": domain, "total": count }),
                format!("Added {domain} to silent dismiss"),
            );
        }
        FederationCommand::Undismiss { domain } => {
            if domain.trim().is_empty() {
                return Err(ChatmailError::config("DOMAIN is required"));
            }
            let remaining = dismiss.remove_count(&pool, domain).await?;
            return out.done_msg(
                format!("Success: Removed '{domain}' from silent dismiss. {remaining} remaining."),
                serde_json::json!({ "domain": domain, "remaining": remaining }),
                format!("Removed {domain} from silent dismiss"),
            );
        }
        FederationCommand::DismissList => {
            let rules = dismiss.list_rules(&pool).await?;
            if out.is_json() {
                let entries: Vec<_> = rules
                    .into_iter()
                    .map(|(domain, created_at)| {
                        serde_json::json!({ "domain": domain, "added": format_created_at(created_at) })
                    })
                    .collect();
                return out.emit(serde_json::json!({ "domains": entries, "total": entries.len() }));
            }
            out.line("[ SILENT DISMISS ]");
            if rules.is_empty() {
                out.line("No domains configured.");
                out.line("---");
                out.line("Total: 0 domains.");
                return Ok(());
            }
            for (i, (domain, created_at)) in rules.iter().enumerate() {
                let date = format_created_at(*created_at);
                out.line(format!("{}. {domain}\t(Added: {date})", i + 1));
            }
            out.line("---");
            out.line(format!("Total: {} domains.", rules.len()));
        }
        FederationCommand::DismissFlush => {
            dismiss.flush(&pool).await?;
            return out.done_msg(
                "WARNING: Silent dismiss list flushed. 0 domains remain.",
                serde_json::json!({ "flushed": true, "total": 0 }),
                "Silent dismiss list flushed",
            );
        }
        FederationCommand::Status => {
            let tracker = FederationTracker::new();
            tracker.hydrate(&pool).await?;
            let stats = tracker.snapshot();
            if out.is_json() {
                let entries: Vec<_> = stats
                    .into_iter()
                    .map(|s| {
                        serde_json::json!({
                            "domain": s.domain,
                            "successful_deliveries": s.successful_deliveries,
                            "queued_messages": s.queued_messages,
                            "failed_http": s.failed_http,
                            "failed_https": s.failed_https,
                            "failed_smtp": s.failed_smtp,
                        })
                    })
                    .collect();
                return out.emit(serde_json::json!({ "traffic": entries }));
            }
            if stats.is_empty() {
                out.line("[ TRAFFIC ANOMALIES ]");
                out.line("No federation traffic recorded.");
                return Ok(());
            }
            out.line("[ TRAFFIC ANOMALIES ]");
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
                out.line(format!(
                    "- {} : {} / {} pending /{}",
                    s.domain, success_info, s.queued_messages, fail_info
                ));
            }
        }
        FederationCommand::Transport { cmd } => {
            return transport_cmd(args, &pool, cmd.as_ref()).await;
        }
    }
    Ok(())
}

async fn transport_cmd(
    args: &Args,
    pool: &DbPool,
    cmd: Option<&FederationTransportCommand>,
) -> Result<()> {
    match cmd {
        None | Some(FederationTransportCommand::Status) => {
            let policy = load_transport_policy(pool).await?;
            let out = CtlOut::from_args(args, "federation transport status");
            if out.is_json() {
                return out.emit(serde_json::json!({
                    "https": policy.mode(TransportName::Https),
                    "http": policy.mode(TransportName::Http),
                    "smtp": policy.mode(TransportName::Smtp),
                }));
            }
            out.line("[ FEDERATION TRANSPORT ]");
            out.line(format!("HTTPS: {}", policy.mode(TransportName::Https)));
            out.line(format!("HTTP:  {}", policy.mode(TransportName::Http)));
            out.line(format!("SMTP:  {}", policy.mode(TransportName::Smtp)));
            out.blank();
            out.line("Unset transports stay allowed.");
            out.line("Apply to a running server: madmail reload");
            Ok(())
        }
        Some(FederationTransportCommand::Set { transport, mode }) => {
            let name = TransportName::parse(transport)?;
            let allow = parse_transport_mode(mode)?;
            set_transport_mode(pool, name, allow).await?;
            let out = CtlOut::from_args(args, "federation transport set");
            let label = if allow { "allowed" } else { "blocked" };
            out.done_msg(
                format!(
                    "Success: outbound {} {label}. Apply to a running server: madmail reload",
                    name.as_str().to_ascii_uppercase()
                ),
                serde_json::json!({
                    "transport": name.as_str(),
                    "mode": if allow { "allow" } else { "block" },
                    "reload_required": true,
                }),
                format!("Outbound {} {label}", name.as_str().to_ascii_uppercase()),
            )
        }
    }
}

fn rule_action(policy: &str) -> &'static str {
    if policy.eq_ignore_ascii_case("REJECT") {
        "allow"
    } else {
        "block"
    }
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
