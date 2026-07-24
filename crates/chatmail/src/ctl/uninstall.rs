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
//!
//! On Windows builds, the Unix/systemd uninstall path is compiled out of use;
//! dead-code lint is silenced for those helpers.

#![cfg_attr(windows, allow(dead_code))]

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use chatmail_config::Args;
use chatmail_config::UninstallArgs;
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;
use super::output::CtlOut;
use super::util;

const DEFAULT_BINARY_NAME: &str = "madmail";

// Unix FHS/systemd uninstall paths are unused when building for Windows.
#[cfg_attr(windows, allow(dead_code))]
const SYSTEMD_UNIT_DIRS: &[&str] = &[
    "/etc/systemd/system",
    "/usr/lib/systemd/system",
    "/lib/systemd/system",
];

#[derive(Debug, Default)]
#[cfg_attr(windows, allow(dead_code))]
struct UninstallPlan {
    installation_found: bool,
    primary_binary_name: String,
    /// systemd unit basenames (without `.service`), e.g. `madmail`, `madmail-new`.
    service_names: Vec<String>,
    service_files: Vec<PathBuf>,
    timer_names: Vec<String>,
    timer_files: Vec<PathBuf>,
    config_dirs: Vec<PathBuf>,
    state_dirs: Vec<PathBuf>,
    binary_paths: Vec<PathBuf>,
    runtime_dirs: Vec<PathBuf>,
    log_dirs: Vec<PathBuf>,
    cache_dirs: Vec<PathBuf>,
    service_users: Vec<String>,
}

pub async fn uninstall(args: &Args, flags: &UninstallArgs) -> Result<()> {
    #[cfg(windows)]
    {
        return uninstall_windows(args, flags).await;
    }

    #[cfg(not(windows))]
    uninstall_unix(args, flags).await
}

#[cfg(not(windows))]
async fn uninstall_unix(args: &Args, flags: &UninstallArgs) -> Result<()> {
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
        if args.json {
            CtlOut::from_args(args, "uninstall").emit(serde_json::json!({ "found": false }))?;
        } else {
            println!("❌ No madmail/chatmail installation detected");
            println!("Nothing to uninstall.");
        }
        return Ok(());
    }

    show_plan(&plan, flags);

    if !flags.force
        && !flags.dry_run
        && !util::confirm(
            "Are you sure you want to proceed with uninstallation",
            false,
        )?
    {
        if args.json {
            CtlOut::from_args(args, "uninstall").aborted()?;
        } else {
            println!("Uninstallation cancelled.");
        }
        return Ok(());
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
        steps.push((
            "Removing state, databases, runtime, logs, and cache",
            remove_data,
        ));
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
    if args.json {
        CtlOut::from_args(args, "uninstall").done_msg(
            "",
            serde_json::json!({
                "uninstalled": true,
                "binary": plan.primary_binary_name,
                "dry_run": flags.dry_run,
                "keep_data": flags.keep_data,
            }),
            "Uninstallation completed successfully",
        )?;
    } else {
        println!("\n🎉 Uninstallation completed successfully!");
        if !flags.keep_data {
            println!("⚠️  All mail data has been permanently deleted.");
        }
    }
    Ok(())
}

/// Windows uninstall: SCM service, firewall rules, ProgramData layout, optional binary.
#[cfg(windows)]
async fn uninstall_windows(args: &Args, flags: &UninstallArgs) -> Result<()> {
    use chatmail_config::{ServiceCommand, DEFAULT_WINDOWS_SERVICE_NAME};

    use super::firewall_cmd;
    use super::service_cmd;

    let ctx = CtlContext::from_args(args)?;
    let primary = current_binary_name();
    let _ = append_log(&flags.log_file, "Starting Windows uninstall");

    println!("🗑️  {primary} — uninstall (Windows)");
    println!("====================================");

    let program_data = std::env::var_os("PROGRAMDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let madmail_root = program_data.join("Madmail");
    let config_dir = madmail_root.join("config");
    let state_dir = if ctx.state_dir.is_dir() {
        ctx.state_dir.clone()
    } else {
        madmail_root.join("data")
    };

    // Always attempt service + firewall cleanup; only skip disk deletes when nothing exists.
    let disk_present = madmail_root.is_dir() || config_dir.is_dir() || state_dir.is_dir();
    if !disk_present && !args.json {
        println!(
            "No ProgramData tree at {} — will still try service + firewall cleanup.",
            madmail_root.display()
        );
    }

    if !args.json {
        println!("Config dir: {}", config_dir.display());
        println!("State dir:  {}", state_dir.display());
        println!("Service:    {DEFAULT_WINDOWS_SERVICE_NAME}");
        println!("Firewall:   remove Madmail rules");
        if flags.dry_run {
            println!("\n(dry-run — no changes will be made)");
        }
    }

    if !flags.force
        && !flags.dry_run
        && !util::confirm(
            "Are you sure you want to proceed with uninstallation",
            false,
        )?
    {
        if args.json {
            CtlOut::from_args(args, "uninstall").aborted()?;
        } else {
            println!("Uninstallation cancelled.");
        }
        return Ok(());
    }

    if flags.dry_run {
        println!("Would: stop/delete service {DEFAULT_WINDOWS_SERVICE_NAME}");
        println!("Would: remove Madmail firewall rules");
        if !flags.keep_config && !flags.keep_data {
            println!("Would: remove entire {}", madmail_root.display());
        } else {
            if !flags.keep_config {
                println!("Would: remove {}", config_dir.display());
            }
            if !flags.keep_data {
                println!("Would: remove {}", state_dir.display());
            }
        }
        if !flags.keep_binary {
            if let Ok(exe) = std::env::current_exe() {
                println!(
                    "Would: leave binary in place unless under Program Files: {}",
                    exe.display()
                );
            }
        }
        if args.json {
            CtlOut::from_args(args, "uninstall").emit(serde_json::json!({
                "uninstalled": true,
                "dry_run": true,
                "keep_data": flags.keep_data,
            }))?;
        } else {
            println!("\n🎉 Dry-run completed.");
        }
        return Ok(());
    }

    println!("\n[1/4] Stopping and removing Windows service...");
    // service uninstall does not need matching config for delete
    let _ = service_cmd::service(
        args,
        &ServiceCommand::Uninstall {
            name: DEFAULT_WINDOWS_SERVICE_NAME.into(),
        },
    )
    .await;
    println!("✅ Service cleanup attempted");

    println!("\n[2/4] Removing firewall rules...");
    match firewall_cmd::remove_rules() {
        Ok(n) => println!("✅ Removed {n} firewall rule(s)"),
        Err(e) => println!("⚠️  Firewall cleanup: {e}"),
    }

    // Prefer removing the whole ProgramData\Madmail tree so install.log, certs,
    // and empty parents are not left behind (remove_dir only works when empty).
    if !flags.keep_config && !flags.keep_data {
        println!("\n[3/4] Removing configuration…");
        println!("\n[4/4] Removing state / mail data and ProgramData\\Madmail…");
        if madmail_root.exists() {
            match remove_path(&madmail_root, false) {
                Ok(()) => println!("✅ Removed {}", madmail_root.display()),
                Err(e) => {
                    // Best-effort partial cleanup if something is locked.
                    println!("⚠️  Full tree remove failed ({e}); trying config + data…");
                    if config_dir.exists() {
                        let _ = remove_path(&config_dir, false);
                    }
                    if state_dir.exists() {
                        let _ = remove_path(&state_dir, false);
                    }
                    let log = madmail_root.join("install.log");
                    if log.exists() {
                        let _ = remove_path(&log, false);
                    }
                    if madmail_root.is_dir() {
                        let _ = fs::remove_dir_all(&madmail_root);
                    }
                }
            }
        } else {
            println!("✅ No ProgramData\\Madmail tree present");
        }
    } else {
        if !flags.keep_config {
            println!("\n[3/4] Removing configuration…");
            if config_dir.exists() {
                remove_path(&config_dir, false)?;
            }
            println!("✅ Configuration removed");
        } else {
            println!("\n[3/4] Keeping configuration (--keep-config)");
        }

        if !flags.keep_data {
            println!("\n[4/4] Removing state / mail data…");
            if state_dir.exists() {
                remove_path(&state_dir, false)?;
            }
            println!("✅ State removed");
        } else {
            println!("\n[4/4] Keeping state / mail data (--keep-data)");
        }

        // Drop install.log and other loose files; remove root if empty.
        let install_log = madmail_root.join("install.log");
        if install_log.exists() && !flags.keep_config {
            let _ = remove_path(&install_log, false);
        }
        if madmail_root.is_dir() {
            match fs::remove_dir(&madmail_root) {
                Ok(()) => println!("✅ Removed empty {}", madmail_root.display()),
                Err(_) => {
                    // Non-empty (e.g. --keep-data left data/) — expected.
                }
            }
        }
    }

    if !flags.keep_binary {
        if let Ok(exe) = std::env::current_exe() {
            let s = exe.to_string_lossy().to_lowercase();
            if s.contains("program files") && s.contains("madmail") {
                println!("Note: remove the install directory via the setup uninstaller when using setup.exe.");
                println!("  binary: {}", exe.display());
            }
        }
    }

    if args.json {
        CtlOut::from_args(args, "uninstall").done_msg(
            "",
            serde_json::json!({
                "uninstalled": true,
                "binary": primary,
                "dry_run": false,
                "keep_data": flags.keep_data,
                "platform": "windows",
            }),
            "Uninstallation completed successfully",
        )?;
    } else {
        println!("\n🎉 Uninstallation completed successfully!");
        if !flags.keep_data {
            println!("⚠️  All mail data has been permanently deleted.");
        }
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
    let mut timer_names = BTreeSet::new();
    let mut timer_files = BTreeSet::new();

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
            if name.ends_with(".service") {
                let stem = name.strip_suffix(".service").unwrap_or(name);
                if !is_family_unit_stem(stem) {
                    continue;
                }
                service_names.insert(stem.to_string());
                service_files.insert(path);
                plan.installation_found = true;
            } else if name.ends_with(".timer") {
                let stem = name.strip_suffix(".timer").unwrap_or(name);
                if !is_family_unit_stem(stem) {
                    continue;
                }
                timer_names.insert(stem.to_string());
                timer_files.insert(path);
                plan.installation_found = true;
            }
        }
    }

    // argv0 unit may not exist on disk yet (e.g. dry-run install); still try stop/disable.
    service_names.insert(primary.to_string());

    for path in &service_files {
        enrich_from_unit_file(&mut plan, path);
    }

    plan.service_names = service_names.into_iter().collect();
    plan.service_files = service_files.into_iter().collect();
    plan.timer_names = timer_names.into_iter().collect();
    plan.timer_files = timer_files.into_iter().collect();

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

    let mut path_prefixes: BTreeSet<String> = plan.service_names.iter().cloned().collect();
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
        if stems
            .iter()
            .any(|s| name == *s || name.starts_with(&format!("{s}-")))
        {
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
        if is_family_unit_stem(name) || name.starts_with("madmail") || name.starts_with("chatmail")
        {
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
    println!("Binary: {}", plan.primary_binary_name);
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
    for name in &plan.timer_names {
        run_systemctl(flags.dry_run, &["stop", name])?;
    }
    for name in &plan.service_names {
        run_systemctl(flags.dry_run, &["stop", name])?;
        if !flags.dry_run {
            println!("Stopped (or already stopped): {name}");
        }
    }
    Ok(())
}

fn disable_services(plan: &UninstallPlan, flags: &UninstallArgs) -> Result<()> {
    for name in &plan.timer_names {
        run_systemctl(flags.dry_run, &["disable", name])?;
    }
    for name in &plan.service_names {
        run_systemctl(flags.dry_run, &["disable", name])?;
    }
    Ok(())
}

fn remove_systemd_files(plan: &UninstallPlan, flags: &UninstallArgs) -> Result<()> {
    for f in &plan.timer_files {
        remove_path(f, flags.dry_run)?;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_config::Args;
    use clap::Parser;

    fn sample_flags(dry_run: bool, log_file: PathBuf) -> UninstallArgs {
        UninstallArgs {
            force: true,
            keep_data: false,
            keep_user: true,
            keep_config: false,
            keep_binary: false,
            dry_run,
            log_file,
        }
    }

    fn ctl_context(state_dir: &Path) -> CtlContext {
        let args =
            Args::try_parse_from(["madmail", "--state-dir", state_dir.to_str().unwrap()]).unwrap();
        CtlContext::from_args(&args).unwrap()
    }

    #[test]
    fn is_family_unit_stem_matches_madmail_and_chatmail_variants() {
        for stem in [
            "madmail",
            "chatmail",
            "madmail-new",
            "madmail_backup",
            "chatmail-dev",
            "chatmail_staging",
        ] {
            assert!(is_family_unit_stem(stem), "expected family stem: {stem}");
        }
        for stem in ["nginx", "madmailx", "postfix", "other-service"] {
            assert!(!is_family_unit_stem(stem), "unexpected family stem: {stem}");
        }
    }

    #[test]
    fn discover_family_dirs_finds_madmail_and_chatmail_prefixes() {
        let parent = tempfile::tempdir().unwrap();
        fs::create_dir(parent.path().join("madmail")).unwrap();
        fs::create_dir(parent.path().join("chatmail-dev")).unwrap();
        fs::create_dir(parent.path().join("unrelated")).unwrap();

        let dirs = discover_family_dirs(parent.path().to_str().unwrap(), &["madmail", "chatmail"]);
        assert_eq!(dirs.len(), 2);
        assert!(dirs.iter().any(|p| p.ends_with("madmail")));
        assert!(dirs.iter().any(|p| p.ends_with("chatmail-dev")));
    }

    #[test]
    fn discover_family_binaries_collects_family_executables() {
        let bin = tempfile::tempdir().unwrap();
        fs::write(bin.path().join("madmail"), b"").unwrap();
        fs::write(bin.path().join("chatmail-helper"), b"").unwrap();
        fs::write(bin.path().join("nginx"), b"").unwrap();

        let mut paths = Vec::new();
        discover_family_binaries(bin.path().to_str().unwrap(), &mut paths);
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|p| p.ends_with("madmail")));
        assert!(paths.iter().any(|p| p.ends_with("chatmail-helper")));
    }

    #[test]
    fn enrich_from_unit_file_extracts_user_workdir_and_execstart() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("madmail");
        fs::write(&binary, b"").unwrap();
        let unit = dir.path().join("madmail.service");
        let unit_body = format!(
            "[Service]\nUser=madmail\nWorkingDirectory={}\nExecStart={} run\n",
            dir.path().display(),
            binary.display()
        );
        fs::write(&unit, unit_body).unwrap();

        let mut plan = UninstallPlan::default();
        enrich_from_unit_file(&mut plan, &unit);

        assert_eq!(plan.service_users, vec!["madmail".to_string()]);
        assert!(plan.state_dirs.iter().any(|p| p == dir.path()));
        assert!(plan.binary_paths.iter().any(|p| p == &binary));
    }

    #[test]
    fn push_unique_helpers_deduplicate_entries() {
        let mut paths = vec![PathBuf::from("/a")];
        push_unique_path(&mut paths, PathBuf::from("/a"));
        push_unique_path(&mut paths, PathBuf::from("/b"));
        assert_eq!(paths, vec![PathBuf::from("/a"), PathBuf::from("/b")]);

        let mut names = vec!["madmail".to_string()];
        push_unique_string(&mut names, "madmail".into());
        push_unique_string(&mut names, "chatmail".into());
        assert_eq!(names, vec!["madmail", "chatmail"]);
    }

    #[test]
    fn remove_path_dry_run_leaves_files_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("keep.txt");
        fs::write(&file, b"x").unwrap();

        remove_path(&file, true).unwrap();
        assert!(file.is_file());
        remove_path(dir.path(), true).unwrap();
        assert!(dir.path().is_dir());
    }

    #[test]
    fn remove_path_deletes_file_and_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("gone.txt");
        fs::write(&file, b"x").unwrap();
        let nested = dir.path().join("nested");
        fs::create_dir(&nested).unwrap();

        remove_path(&file, false).unwrap();
        assert!(!file.exists());

        remove_path(&nested, false).unwrap();
        assert!(!nested.exists());
    }

    #[test]
    fn run_systemctl_dry_run_does_not_invoke_systemctl() {
        let plan = UninstallPlan {
            service_names: vec!["madmail".into()],
            ..Default::default()
        };
        let flags = sample_flags(true, PathBuf::from("/tmp/uninstall-test.log"));
        stop_services(&plan, &flags).unwrap();
        disable_services(&plan, &flags).unwrap();
        daemon_reload(true).unwrap();
    }

    #[test]
    fn append_log_writes_message() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("uninstall.log");
        append_log(&log, "test entry").unwrap();
        let content = fs::read_to_string(&log).unwrap();
        assert!(content.contains("test entry"));
    }

    #[test]
    fn detect_installation_finds_state_dir_from_context() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctl_context(dir.path());
        let plan = detect_installation(&ctx, "madmail").unwrap();

        assert!(plan.installation_found);
        assert!(plan.state_dirs.iter().any(|p| p == &ctx.state_dir));
        assert!(plan.service_names.contains(&"madmail".to_string()));
    }

    #[test]
    fn detect_installation_not_found_without_state_or_artifacts() {
        let base = tempfile::tempdir().unwrap();
        let missing = base.path().join("missing-state");
        let ctx = ctl_context(&missing);
        let plan = detect_installation(&ctx, "madmail-test-only").unwrap();

        assert!(!plan.installation_found);
        assert!(plan
            .service_names
            .contains(&"madmail-test-only".to_string()));
    }

    #[tokio::test]
    async fn uninstall_dry_run_completes_when_state_dir_exists() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("uninstall.log");
        let args =
            Args::try_parse_from(["madmail", "--state-dir", dir.path().to_str().unwrap()]).unwrap();
        let flags = sample_flags(true, log.clone());

        uninstall(&args, &flags).await.unwrap();
        assert!(log.is_file());
    }

    #[tokio::test]
    async fn uninstall_reports_not_found_when_nothing_detected() {
        let base = tempfile::tempdir().unwrap();
        let missing = base.path().join("no-install");
        let log = base.path().join("log");
        let args = Args::try_parse_from([
            "madmail",
            "--json",
            "--state-dir",
            missing.to_str().unwrap(),
        ])
        .unwrap();
        let flags = UninstallArgs::parse_from([
            "uninstall",
            "--dry-run",
            "--force",
            "--log-file",
            log.to_str().unwrap(),
        ]);

        uninstall(&args, &flags).await.unwrap();
    }

    #[tokio::test]
    async fn uninstall_requires_root_without_dry_run() {
        if is_root() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let args =
            Args::try_parse_from(["madmail", "--state-dir", dir.path().to_str().unwrap()]).unwrap();
        let flags = sample_flags(false, dir.path().join("uninstall.log"));

        let err = uninstall(&args, &flags).await.unwrap_err();
        assert!(err.to_string().contains("root"));
    }

    #[test]
    fn dry_run_remove_steps_leave_artifacts_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        let state = dir.path().join("state");
        let binary = dir.path().join("madmail");
        fs::create_dir_all(&config).unwrap();
        fs::create_dir_all(&state).unwrap();
        fs::write(&binary, b"").unwrap();

        let plan = UninstallPlan {
            installation_found: true,
            primary_binary_name: "madmail".into(),
            config_dirs: vec![config.clone()],
            state_dirs: vec![state.clone()],
            binary_paths: vec![binary.clone()],
            ..Default::default()
        };

        let flags = sample_flags(true, dir.path().join("log"));

        remove_config(&plan, &flags).unwrap();
        remove_data(&plan, &flags).unwrap();
        remove_binaries(&plan, &flags).unwrap();

        assert!(config.is_dir());
        assert!(state.is_dir());
        assert!(binary.is_file());
    }
}
