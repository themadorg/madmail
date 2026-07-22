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

//! `madmail firewall` — Windows Firewall inbound rules for mail/HTTP listeners.

use chatmail_config::{Args, FirewallCommand, FIREWALL_RULE_PREFIX};
use chatmail_types::{ChatmailError, Result};

use super::output::CtlOut;

/// Core ports always opened by `firewall apply`.
const CORE_RULES: &[(&str, &str, u16)] = &[
    ("SMTP", "TCP", 25),
    ("IMAP", "TCP", 143),
    ("Submission", "TCP", 587),
    ("SubmissionTLS", "TCP", 465),
    ("IMAPTLS", "TCP", 993),
    ("HTTP", "TCP", 80),
    ("HTTPS", "TCP", 443),
];

pub async fn firewall(args: &Args, cmd: &FirewallCommand) -> Result<()> {
    match cmd {
        FirewallCommand::Apply { turn, ss, iroh } => apply(args, *turn, *ss, *iroh),
        FirewallCommand::Remove => remove(args),
    }
}

/// Open standard Madmail ports (and optional TURN / SS / Iroh).
pub fn apply_rules(turn: bool, ss: bool, iroh: bool) -> Result<Vec<String>> {
    require_windows()?;
    let mut names = Vec::new();
    for (label, proto, port) in CORE_RULES {
        let name = rule_name(label);
        add_rule(&name, proto, *port)?;
        names.push(name);
    }
    if turn {
        let name = rule_name("TURN");
        add_rule(&name, "UDP", 3478)?;
        names.push(name.clone());
        // TCP TURN as well (config often binds both).
        let name_tcp = rule_name("TURN-TCP");
        add_rule(&name_tcp, "TCP", 3478)?;
        names.push(name_tcp);
    }
    if ss {
        let name = rule_name("Shadowsocks");
        add_rule(&name, "TCP", 8388)?;
        names.push(name);
    }
    if iroh {
        let name = rule_name("Iroh");
        add_rule(&name, "TCP", 3340)?;
        names.push(name);
    }
    Ok(names)
}

/// Delete all inbound rules with the Madmail display-name prefix.
pub fn remove_rules() -> Result<usize> {
    require_windows()?;
    let listed = list_madmail_rules()?;
    let mut removed = 0usize;
    for name in &listed {
        if delete_rule(name).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn apply(args: &Args, turn: bool, ss: bool, iroh: bool) -> Result<()> {
    let out = CtlOut::from_args(args, "firewall");
    let names = apply_rules(turn, ss, iroh)?;
    if out.is_json() {
        out.emit(serde_json::json!({
            "applied": true,
            "rules": names,
            "turn": turn,
            "ss": ss,
            "iroh": iroh,
        }))?;
    } else {
        println!("✓ Windows Firewall: {} rule(s)", names.len());
        for n in &names {
            println!("  • {n}");
        }
    }
    Ok(())
}

fn remove(args: &Args) -> Result<()> {
    let out = CtlOut::from_args(args, "firewall");
    let n = remove_rules()?;
    if out.is_json() {
        out.emit(serde_json::json!({ "removed": n }))?;
    } else {
        println!("✓ Removed {n} Madmail firewall rule(s)");
    }
    Ok(())
}

fn rule_name(label: &str) -> String {
    format!("{FIREWALL_RULE_PREFIX} ({label})")
}

fn require_windows() -> Result<()> {
    #[cfg(windows)]
    {
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err(ChatmailError::config(
            "madmail firewall is only supported on Windows\n\
             On Linux open ports with your distro firewall (firewalld/ufw/nftables).",
        ))
    }
}

#[cfg(windows)]
fn add_rule(name: &str, protocol: &str, port: u16) -> Result<()> {
    use std::process::Command;
    // Idempotent: delete existing same-name rule first.
    let _ = delete_rule(name);
    let status = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name={name}"),
            "dir=in",
            "action=allow",
            &format!("protocol={protocol}"),
            &format!("localport={port}"),
            "profile=any",
            "enable=yes",
        ])
        .status()
        .map_err(|e| ChatmailError::config(format!("netsh add rule: {e}")))?;
    if !status.success() {
        return Err(ChatmailError::config(format!(
            "netsh add rule failed for {name} (exit {:?})",
            status.code()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn add_rule(_name: &str, _protocol: &str, _port: u16) -> Result<()> {
    require_windows()
}

#[cfg(windows)]
fn delete_rule(name: &str) -> Result<()> {
    use std::process::Command;
    let status = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            &format!("name={name}"),
        ])
        .status()
        .map_err(|e| ChatmailError::config(format!("netsh delete rule: {e}")))?;
    if !status.success() {
        return Err(ChatmailError::config(format!(
            "netsh delete rule failed for {name} (exit {:?})",
            status.code()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn delete_rule(_name: &str) -> Result<()> {
    require_windows()
}

#[cfg(windows)]
fn list_madmail_rules() -> Result<Vec<String>> {
    use std::process::Command;
    let output = Command::new("netsh")
        .args(["advfirewall", "firewall", "show", "rule", "name=all"])
        .output()
        .map_err(|e| ChatmailError::config(format!("netsh show rule: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut names = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        // English and some locales: "Rule Name:                Madmail (SMTP)"
        if let Some(rest) = line
            .strip_prefix("Rule Name:")
            .or_else(|| line.strip_prefix("Rule name:"))
        {
            let name = rest.trim();
            if name.starts_with(FIREWALL_RULE_PREFIX) {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

#[cfg(not(windows))]
fn list_madmail_rules() -> Result<Vec<String>> {
    require_windows().map(|_| unreachable!())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_names_use_prefix() {
        assert_eq!(rule_name("SMTP"), "Madmail (SMTP)");
        assert!(rule_name("HTTPS").starts_with(FIREWALL_RULE_PREFIX));
    }

    #[test]
    fn core_rules_cover_mail_and_http() {
        let ports: Vec<u16> = CORE_RULES.iter().map(|(_, _, p)| *p).collect();
        for p in [25u16, 143, 587, 465, 993, 80, 443] {
            assert!(ports.contains(&p), "missing port {p}");
        }
    }

    #[tokio::test]
    async fn firewall_errors_on_non_windows() {
        #[cfg(not(windows))]
        {
            let args = Args {
                config: std::path::PathBuf::from("/tmp/x.conf"),
                state_dir: std::path::PathBuf::from("/tmp/x"),
                boot_once: false,
                json: false,
            };
            let err = firewall(&args, &FirewallCommand::Remove).await.unwrap_err();
            assert!(
                err.to_string().contains("only supported on Windows"),
                "{err}"
            );
        }
    }
}
