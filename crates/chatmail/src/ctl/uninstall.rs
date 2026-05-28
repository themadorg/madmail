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

//! `chatmail uninstall` — Madmail `ctl/uninstall.go` (systemd + FHS paths).

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use chatmail_config::UninstallArgs;
use chatmail_config::Args;
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;
use super::util;

const DEFAULT_BINARY_NAME: &str = "madmail";

const SYSTEMD_UNIT_DIRS: &[&str] = &[
    "/etc/systemd/system",
    "/usr/lib/systemd/system",
    "/lib/systemd/system",
];

#[derive(Debug, Default)]
struct UninstallPlan {
    installation_found: bool,
    #[allow(dead_code)]
    primary_binary_name: String,
    /// systemd unit basenames (without `.service`), e.g. `madmail`, `madmail-new`.
    service_names: Vec<String>,
    service_files: Vec<PathBuf>,
    config_dirs: Vec<PathBuf>,
    state_dirs: Vec<PathBuf>,
    binary_paths: Vec<PathBuf>,
    runtime_dirs: Vec<PathBuf>,
    log_dirs: Vec<PathBuf>,
    cache_dirs: Vec<PathBuf>,
    service_users: Vec<String>,
}

pub async fn uninstall(args: &Args, flags: &UninstallArgs) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;

    if !flags.dry_run && !is_root() {
        return Err(ChatmailError::config(
            "uninstallation must be run as root (use sudo)",
        ));
    }

    let primary = current_binary_name();
    append_log(&flags.log_file, "Starting uninstall")?;

    println!("🗑️  {primary} — uninstall");
    println!("====================================");

    let plan = detect_installation(&ctx, &primary)?;

    if !plan.installation_found {
        println!("❌ No madmail/chatmail installation detected");
        println!("Nothing to uninstall.");
        return Ok(());
    }

    show_plan(&plan, flags);

    if !flags.force && !flags.dry_run {
        if !util::confirm(
            "Are you sure you want to proceed with uninstallation",
            false,
        )? {
            println!("Uninstallation cancelled.");
            return Ok(());
        }
    }

    let steps: Vec<(&str, StepFn)> = vec![
        ("Stopping services", stop_services),
        ("Disabling services", disable_services),
        ("Removing systemd service files", remove_systemd_files),
    ];

    let mut steps = steps;
    if !flags.keep_config {
        steps.push(("Removing configuration files", remove_config));
    }
    if !flags.keep_data {
        steps.push(("Removing state, databases, runtime, logs, and cache", remove_data));
    }
    if !flags.keep_binary {
        steps.push(("Removing binaries", remove_binaries));
    }
    if !flags.keep_user {
        steps.push(("Removing service users and groups", remove_users));
    }

    for (i, (name, step)) in steps.iter().enumerate() {
        println!("\n[{}/{}] {name}...", i + 1, steps.len());
        step(&plan, flags)?;
        println!("✅ {name} completed");
    }

    daemon_reload(flags.dry_run)?;
    println!("\n🎉 Uninstallation completed successfully!");
    if !flags.keep_data {
        println!("⚠️  All mail data has been permanently deleted.");
    }
    Ok(())
}

type StepFn = fn(&UninstallPlan, &UninstallArgs) -> Result<()>;

fn current_binary_name() -> String {
    std::env::args()
        .next()
        .and_then(|p| {
            Path::new(&p)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| DEFAULT_BINARY_NAME.into())
}

fn is_family_unit_stem(stem: &str) -> bool {
    stem == "madmail"
        || stem == "chatmail"
        || stem.starts_with("madmail-")
        || stem.starts_with("madmail_")
        || stem.starts_with("chatmail-")
        || stem.starts_with("chatmail_")
}

fn detect_installation(ctx: &CtlContext, primary: &str) -> Result<UninstallPlan> {
    let mut plan = UninstallPlan {
        primary_binary_name: primary.to_string(),
        ..Default::default()
    };

    let mut service_names = BTreeSet::new();
    let mut service_files = BTreeSet::new();

    for base in SYSTEMD_UNIT_DIRS {
        let dir = Path::new(base);
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with(".service") {
                continue;
            }
            let stem = name.strip_suffix(".service").unwrap_or(name);
            if !is_family_unit_stem(stem) {
                continue;
            }
            service_names.insert(stem.to_string());
            service_files.insert(path);
            plan.installation_found = true;
        }
    }

    // argv0 unit may not exist on disk yet (e.g. dry-run install); still try stop/disable.
    service_names.insert(primary.to_string());

    for path in &service_files {
        enrich_from_unit_file(&mut plan, path);
    }

    plan.service_names = service_names.into_iter().collect();
    plan.service_files = service_files.into_iter().collect();

    for dir in discover_family_dirs("/etc", &["madmail", "chatmail"]) {
        push_unique_path(&mut plan.config_dirs, dir);
        plan.installation_found = true;
    }

    for prefix in ["/usr/local/bin", "/usr/bin"] {
        discover_family_binaries(prefix, &mut plan.binary_paths);
    }
    if !plan.binary_paths.is_empty() {
        plan.installation_found = true;
    }

    if ctx.state_dir.is_dir() {
        push_unique_path(&mut plan.state_dirs, ctx.state_dir.clone());
        plan.installation_found = true;
    }
    for dir in discover_family_dirs("/var/lib", &["madmail", "chatmail"]) {
        push_unique_path(&mut plan.state_dirs, dir);
        plan.installation_found = true;
    }

    let mut path_prefixes: BTreeSet<String> = plan
        .service_names
        .iter()
        .cloned()
        .collect();
    for sd in &plan.state_dirs {
        if let Some(name) = sd.file_name().and_then(|n| n.to_str()) {
            path_prefixes.insert(name.to_string());
        }
    }
    path_prefixes.insert(DEFAULT_BINARY_NAME.into());
    path_prefixes.insert(primary.to_string());

    for prefix in path_prefixes {
        for (vec, base) in [
            (&mut plan.runtime_dirs, "/run"),
            (&mut plan.log_dirs, "/var/log"),
            (&mut plan.cache_dirs, "/var/cache"),
        ] {
            let p = PathBuf::from(format!("{base}/{prefix}"));
            if p.exists() {
                push_unique_path(vec, p);
            }
        }
    }

    let mut users: BTreeSet<String> = plan.service_users.iter().cloned().collect();
    for name in &plan.service_names {
        if user_exists(name) {
            users.insert(name.clone());
        }
    }
    plan.service_users = users.into_iter().collect();
    if !plan.service_users.is_empty() {
        plan.installation_found = true;
    }

    if plan.config_dir_present() || !plan.binary_paths.is_empty() {
        plan.installation_found = true;
    }

    Ok(plan)
}

impl UninstallPlan {
    fn config_dir_present(&self) -> bool {
        !self.config_dirs.is_empty()
    }
}

fn enrich_from_unit_file(plan: &mut UninstallPlan, path: &Path) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    for line in content.lines() {
        let line = line.trim();
        if let Some(user) = line.strip_prefix("User=") {
            let user = user.trim();
            if !user.is_empty() {
                push_unique_string(&mut plan.service_users, user.to_string());
            }
        }
        if let Some(dir) = line.strip_prefix("WorkingDirectory=") {
            let dir = dir.trim();
            if !dir.is_empty() {
                push_unique_path(&mut plan.state_dirs, PathBuf::from(dir));
            }
        }
        if let Some(exec) = line.strip_prefix("ExecStart=") {
            let binary = exec.split_whitespace().next().unwrap_or(exec).trim();
            if !binary.is_empty() && Path::new(binary).is_file() {
                push_unique_path(&mut plan.binary_paths, PathBuf::from(binary));
            }
        }
    }
}

fn discover_family_dirs(parent: &str, stems: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let parent = Path::new(parent);
    let Ok(entries) = fs::read_dir(parent) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if stems.iter().any(|s| name == *s || name.starts_with(&format!("{s}-"))) {
            out.push(path);
        }
    }
    out
}

fn discover_family_binaries(prefix: &str, out: &mut Vec<PathBuf>) {
    let dir = Path::new(prefix);
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if is_family_unit_stem(name) || name.starts_with("madmail") || name.starts_with("chatmail") {
            push_unique_path(out, path);
        }
    }
}

fn push_unique_path(vec: &mut Vec<PathBuf>, path: PathBuf) {
    if !vec.iter().any(|p| p == &path) {
        vec.push(path);
    }
}

fn push_unique_string(vec: &mut Vec<String>, s: String) {
    if !vec.iter().any(|x| x == &s) {
        vec.push(s);
    }
}

fn show_plan(plan: &UninstallPlan, flags: &UninstallArgs) {
    println!("\n📋 Uninstallation Plan");
    println!("======================");
    if !plan.service_names.is_empty() {
        println!("Services: {}", plan.service_names.join(", "));
    }
    for sd in &plan.state_dirs {
        println!("State dir: {}", sd.display());
    }
    if !flags.keep_config {
        for cd in &plan.config_dirs {
            println!("Config dir: {}", cd.display());
        }
    }
    if !flags.keep_binary {
        for bp in &plan.binary_paths {
            println!("Binary: {}", bp.display());
        }
    }
    for f in &plan.service_files {
        println!("Systemd unit: {}", f.display());
    }
    if flags.dry_run {
        println!("\n(dry-run — no files will be removed)");
    }
}

fn stop_services(plan: &UninstallPlan, flags: &UninstallArgs) -> Result<()> {
    for name in &plan.service_names {
        run_systemctl(flags.dry_run, &["stop", name])?;
        if !flags.dry_run {
            println!("Stopped (or already stopped): {name}");
        }
    }
    Ok(())
}

fn disable_services(plan: &UninstallPlan, flags: &UninstallArgs) -> Result<()> {
    for name in &plan.service_names {
        run_systemctl(flags.dry_run, &["disable", name])?;
    }
    Ok(())
}

fn remove_systemd_files(plan: &UninstallPlan, flags: &UninstallArgs) -> Result<()> {
    for f in &plan.service_files {
        remove_path(f, flags.dry_run)?;
    }
    Ok(())
}

fn remove_config(plan: &UninstallPlan, flags: &UninstallArgs) -> Result<()> {
    for dir in &plan.config_dirs {
        remove_path(dir, flags.dry_run)?;
    }
    Ok(())
}

fn remove_data(plan: &UninstallPlan, flags: &UninstallArgs) -> Result<()> {
    for dir in &plan.state_dirs {
        remove_path(dir, flags.dry_run)?;
    }
    for dir in &plan.log_dirs {
        remove_path(dir, flags.dry_run)?;
    }
    for dir in &plan.cache_dirs {
        remove_path(dir, flags.dry_run)?;
    }
    for dir in &plan.runtime_dirs {
        remove_path(dir, flags.dry_run)?;
    }
    Ok(())
}

fn remove_binaries(plan: &UninstallPlan, flags: &UninstallArgs) -> Result<()> {
    for p in &plan.binary_paths {
        remove_path(p, flags.dry_run)?;
    }
    Ok(())
}

fn remove_users(plan: &UninstallPlan, flags: &UninstallArgs) -> Result<()> {
    for user in &plan.service_users {
        if flags.dry_run {
            println!("Would remove user: {user}");
            continue;
        }
        let status = Command::new("userdel").args(["-r", user]).status();
        if status.is_err() || !status.unwrap().success() {
            let _ = Command::new("userdel").arg(user).status();
        }
        println!("Removed user (if existed): {user}");
    }
    Ok(())
}

fn daemon_reload(dry_run: bool) -> Result<()> {
    run_systemctl(dry_run, &["daemon-reload"])
}

fn run_systemctl(dry_run: bool, args: &[&str]) -> Result<()> {
    if dry_run {
        println!("Would run: systemctl {}", args.join(" "));
        return Ok(());
    }
    let status = Command::new("systemctl").args(args).status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => {
            println!(
                "ℹ️  systemctl {}: non-zero exit (continuing)",
                args.join(" ")
            );
            Ok(())
        }
        Err(e) => Err(ChatmailError::config(format!("systemctl failed: {e}"))),
    }
}

fn remove_path(path: &Path, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("Would remove: {}", path.display());
        return Ok(());
    }
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| {
            ChatmailError::config(format!("failed to remove {}: {e}", path.display()))
        })?;
    } else if path.exists() {
        fs::remove_file(path).map_err(|e| {
            ChatmailError::config(format!("failed to remove {}: {e}", path.display()))
        })?;
    }
    println!("Removed: {}", path.display());
    Ok(())
}

fn user_exists(name: &str) -> bool {
    Command::new("id")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[cfg(not(unix))]
fn is_root() -> bool {
    false
}

fn append_log(path: &Path, msg: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| ChatmailError::config(format!("log file: {e}")))?;
    let now = time::OffsetDateTime::now_utc();
    let ts = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    writeln!(f, "[{ts}] {msg}").ok();
    Ok(())
}
