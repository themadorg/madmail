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
use super::host_paths::{is_family_stem, is_named_for_madmail, normalize, DENIED_DIRS};
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
    /// Directories removed with all their contents.
    config_dirs: Vec<PathBuf>,
    /// Files removed from a directory that is otherwise left in place.
    config_files: Vec<PathBuf>,
    /// Directories removed with all their contents.
    state_dirs: Vec<PathBuf>,
    /// Paths deliberately left in place, with the reason shown to the operator.
    kept: Vec<Kept>,
    /// Config files found by any source (unit `--config`, `/etc` discovery),
    /// with the binary name they belong to; classified by [`plan_paths`].
    config_candidates: Vec<(PathBuf, Option<String>)>,
    /// State directories found by any source (the state directory in use,
    /// `/var/lib` discovery, unit `WorkingDirectory=`); classified by
    /// [`plan_paths`].
    state_candidates: Vec<PathBuf>,
    /// For each directory in `config_dirs`, the config file install wrote in it,
    /// removed alone if the directory must be kept after all.
    config_dir_files: Vec<(PathBuf, PathBuf)>,
    binary_paths: Vec<PathBuf>,
    runtime_dirs: Vec<PathBuf>,
    log_dirs: Vec<PathBuf>,
    cache_dirs: Vec<PathBuf>,
    service_users: Vec<String>,
}

/// Which removal step a kept path belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeptKind {
    Config,
    State,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Kept {
    kind: KeptKind,
    path: PathBuf,
    reason: String,
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

    let (state_dir, untrusted) = uninstall_state_dir(args, &ctx);
    let plan = detect_installation(&state_dir, untrusted, &primary, flags)?;

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
    let kept = kept_paths(&plan, flags);
    if args.json {
        let left: Vec<_> = kept
            .iter()
            .map(
                |k| serde_json::json!({ "path": k.path.display().to_string(), "reason": k.reason }),
            )
            .collect();
        CtlOut::from_args(args, "uninstall").done_msg(
            "",
            serde_json::json!({
                "uninstalled": true,
                "binary": plan.primary_binary_name,
                "dry_run": flags.dry_run,
                "keep_data": flags.keep_data,
                "left_in_place": left,
            }),
            "Uninstallation completed successfully",
        )?;
    } else {
        println!("\n🎉 Uninstallation completed successfully!");
        if !flags.keep_data {
            println!(
                "⚠️  All mail data in the removed state directories has been permanently deleted."
            );
        }
        for k in kept {
            println!("Left in place: {} ({})", k.path.display(), k.reason);
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

/// The state directory uninstall works on. A `state_dir` set in the config
/// file is used only when every component of the config file's path, and the
/// file itself, is owned by root and not writable by another user or group;
/// otherwise anyone who can edit that file could point uninstall at any
/// directory. An ignored value is returned as a kept entry, and the
/// `--state-dir` value is used instead.
fn uninstall_state_dir(args: &Args, ctx: &CtlContext) -> (PathBuf, Option<Kept>) {
    let Some(from_config) = ctx.config.state_dir.clone() else {
        return (ctx.state_dir.clone(), None);
    };
    match config_file_untrusted(&args.config) {
        None => (from_config, None),
        Some(why) => {
            let kept = Kept {
                kind: KeptKind::State,
                path: from_config,
                reason: format!(
                    "named by state_dir in {}, which is not used: {why}",
                    args.config.display()
                ),
            };
            (args.state_dir.clone(), Some(kept))
        }
    }
}

/// Why the config file at `path` cannot be trusted to name paths for removal,
/// or `None` when it can.
#[cfg(unix)]
fn config_file_untrusted(path: &Path) -> Option<String> {
    use super::secure_fs::{Resolved, Resolver, Trust, Want};

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(e) => return Some(format!("cannot resolve {}: {e}", path.display())),
        }
    };
    let trust = Trust::root_or_caller();
    let resolver = Resolver {
        trust: &trust,
        state_root: None,
        check_final: true,
    };
    match resolver.resolve(&absolute, Want::File) {
        Ok(Resolved::Found(_)) => None,
        Ok(Resolved::Controlled { component, reason }) => {
            Some(format!("{} is {reason}", component.display()))
        }
        Ok(Resolved::Missing { .. }) | Ok(Resolved::InTree(_)) => {
            Some(format!("{} does not exist", absolute.display()))
        }
        Err(e) => Some(format!("cannot open {}: {e}", absolute.display())),
    }
}

#[cfg(not(unix))]
fn config_file_untrusted(_path: &Path) -> Option<String> {
    None
}

fn detect_installation(
    state_dir: &Path,
    untrusted: Option<Kept>,
    primary: &str,
    flags: &UninstallArgs,
) -> Result<UninstallPlan> {
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
                if !is_family_stem(stem) {
                    continue;
                }
                service_names.insert(stem.to_string());
                service_files.insert(path);
                plan.installation_found = true;
            } else if name.ends_with(".timer") {
                let stem = name.strip_suffix(".timer").unwrap_or(name);
                if !is_family_stem(stem) {
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
        // Install writes `<binary>.conf` into the config directory named for it.
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let entry = (dir.join(format!("{name}.conf")), Some(name));
        if !plan.config_candidates.contains(&entry) {
            plan.config_candidates.push(entry);
        }
        plan.installation_found = true;
    }

    for prefix in ["/usr/local/bin", "/usr/bin"] {
        discover_family_binaries(prefix, &mut plan.binary_paths);
    }
    if !plan.binary_paths.is_empty() {
        plan.installation_found = true;
    }

    if state_dir.is_dir() {
        push_unique_path(&mut plan.state_candidates, state_dir.to_path_buf());
        plan.installation_found = true;
    }
    for dir in discover_family_dirs("/var/lib", &["madmail", "chatmail"]) {
        push_unique_path(&mut plan.state_candidates, dir);
        plan.installation_found = true;
    }

    let deny = DenyList::from_system(&plan.service_users);
    plan_paths(&mut plan, &deny);

    let mut path_prefixes: BTreeSet<String> = plan.service_names.iter().cloned().collect();
    for sd in &plan.state_dirs {
        if let Some(name) = sd.file_name().and_then(|n| n.to_str()) {
            // `/run`, `/var/log` and `/var/cache` entries are only derived from
            // names that clearly belong to madmail.
            if is_family_stem(name) || name == primary {
                path_prefixes.insert(name.to_string());
            }
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

    protect_preserved(&mut plan, flags.keep_data, flags.keep_config);
    // Only a note: the directory may still be found (and removed) another way.
    if let Some(kept) = untrusted {
        if !plan.state_dirs.contains(&kept.path) {
            add_kept(&mut plan, kept.kind, &kept.path.clone(), kept.reason);
        }
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
            // Written verbatim by install; only specifiers are escaped.
            let dir = dir.trim().replace("%%", "%");
            if !dir.is_empty() {
                push_unique_path(&mut plan.state_candidates, PathBuf::from(dir));
            }
        }
        if let Some(exec) = line.strip_prefix("ExecStart=") {
            let words = split_exec_words(exec);
            if let Some(binary) = words.first() {
                if Path::new(binary).is_file() {
                    push_unique_path(&mut plan.binary_paths, PathBuf::from(binary));
                }
            }
            // Custom-path installs keep config (secrets, TLS key) outside /etc.
            let config = words.iter().enumerate().find_map(|(i, w)| {
                if w == "--config" {
                    words.get(i + 1).cloned()
                } else {
                    w.strip_prefix("--config=").map(str::to_string)
                }
            });
            if let Some(config) = config.filter(|c| Path::new(c).is_absolute()) {
                let binary = words
                    .first()
                    .and_then(|b| Path::new(b).file_name())
                    .map(|n| n.to_string_lossy().into_owned());
                let entry = (PathBuf::from(config), binary);
                if !plan.config_candidates.contains(&entry) {
                    plan.config_candidates.push(entry);
                }
            }
        }
    }
}

/// Directories uninstall must never remove as a whole: [`DENIED_DIRS`], any
/// direct child of `/home` or `/Users`, and the homes of real users.
#[derive(Debug, Default)]
struct DenyList {
    homes: Vec<PathBuf>,
}

impl DenyList {
    fn new(homes: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            homes: homes.into_iter().map(|h| normalize(&h)).collect(),
        }
    }

    /// Homes of root and of regular accounts (uid >= 1000, except `nobody`) from
    /// `/etc/passwd`, plus `$HOME` and the `sudo` caller's home. Service accounts
    /// being removed are excluded: their home is the state directory.
    fn from_system(service_users: &[String]) -> Self {
        let mut homes = Vec::new();
        if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
            homes.extend(real_user_homes(&passwd, service_users));
        }
        if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
            homes.push(PathBuf::from(home));
        }
        if let Some(user) = std::env::var("SUDO_USER").ok().filter(|u| !u.is_empty()) {
            if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
                homes.extend(passwd.lines().filter_map(|line| {
                    let fields: Vec<&str> = line.split(':').collect();
                    (fields.len() >= 6 && fields[0] == user).then(|| PathBuf::from(fields[5]))
                }));
            }
        }
        Self::new(homes)
    }

    /// Why `path` must not be removed as a whole, or `None` when it may be.
    /// Both the path as written (normalized) and its resolved form are checked.
    fn reason(&self, path: &Path) -> Option<&'static str> {
        let mut forms = vec![normalize(path)];
        if let Ok(resolved) = fs::canonicalize(path) {
            forms.push(resolved);
        }
        forms.iter().find_map(|form| {
            if DENIED_DIRS.iter().any(|d| form == Path::new(d)) {
                return Some("well-known system directory");
            }
            let parent = form.parent();
            if parent == Some(Path::new("/home")) || parent == Some(Path::new("/Users")) {
                return Some("user home directory");
            }
            if self.homes.iter().any(|h| h == form) {
                return Some("user home directory");
            }
            None
        })
    }
}

fn real_user_homes(passwd: &str, service_users: &[String]) -> Vec<PathBuf> {
    passwd
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() < 6 || service_users.iter().any(|u| u == fields[0]) {
                return None;
            }
            let uid: u32 = fields[2].parse().ok()?;
            let regular = uid == 0 || (uid >= 1000 && uid != 65534);
            (regular && fields[5].starts_with('/')).then(|| PathBuf::from(fields[5]))
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
enum DirClass {
    /// Named for madmail (`madmail*`, `chatmail*` or this binary's name).
    Dedicated,
    /// Any other directory: it may hold files that are not madmail's.
    Shared,
    /// Never removed as a whole.
    Denied(&'static str),
}

fn classify_dir(path: &Path, primary: &str, deny: &DenyList) -> DirClass {
    if let Some(reason) = deny.reason(path) {
        return DirClass::Denied(reason);
    }
    if is_named_for_madmail(path, primary) {
        DirClass::Dedicated
    } else {
        DirClass::Shared
    }
}

/// Whether `config` has the name install gives its config file: `<binary>.conf`
/// for the unit's binary, this binary or a madmail/chatmail name.
fn is_install_config_file(config: &Path, binary: Option<&str>, primary: &str) -> bool {
    let Some(name) = config.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(stem) = name.strip_suffix(".conf") else {
        return false;
    };
    binary == Some(stem) || stem == primary || is_family_stem(stem)
}

/// Decide what to remove for every config and state directory found, whatever
/// found it: the state directory in use (from `--state-dir` or a trusted
/// config file), `/etc` and `/var/lib` discovery, and units (`--config`,
/// `WorkingDirectory=`).
///
/// A directory is removed as a whole only when it is named for madmail and not
/// on the deny list. From any other config directory only the config file
/// install wrote there is removed; TLS files in it are kept because nothing
/// says whether install created them. Any other state directory is kept: the
/// service's data may be mixed with other files there.
fn plan_paths(plan: &mut UninstallPlan, deny: &DenyList) {
    let primary = plan.primary_binary_name.clone();

    for (config, binary) in std::mem::take(&mut plan.config_candidates) {
        let Some(dir) = config.parent().filter(|d| d.is_absolute()) else {
            continue;
        };
        let install_named = is_install_config_file(&config, binary.as_deref(), &primary);
        let why = match classify_dir(dir, &primary, deny) {
            DirClass::Dedicated => {
                push_unique_path(&mut plan.config_dirs, dir.to_path_buf());
                if install_named {
                    let entry = (dir.to_path_buf(), config.clone());
                    if !plan.config_dir_files.contains(&entry) {
                        plan.config_dir_files.push(entry);
                    }
                }
                continue;
            }
            DirClass::Shared => "shared directory, not named for madmail",
            DirClass::Denied(reason) => reason,
        };
        let file = config.file_name().unwrap_or_default().to_string_lossy();
        if install_named {
            push_unique_path(&mut plan.config_files, config.clone());
            add_kept(
                plan,
                KeptKind::Config,
                dir,
                format!("{why}; only {file} is removed"),
            );
        } else {
            add_kept(
                plan,
                KeptKind::Config,
                &config,
                format!("{why}; {file} is not a config file name install writes"),
            );
        }
        for name in ["fullchain.pem", "privkey.pem"] {
            let tls = dir.join("certs").join(name);
            if fs::symlink_metadata(&tls).is_ok() {
                add_kept(
                    plan,
                    KeptKind::Config,
                    &tls,
                    "TLS file in a shared directory; install may not have created it, remove it yourself if it did".into(),
                );
            }
        }
    }

    for dir in std::mem::take(&mut plan.state_candidates) {
        if !dir.is_absolute() {
            continue;
        }
        match classify_dir(&dir, &primary, deny) {
            DirClass::Dedicated => push_unique_path(&mut plan.state_dirs, dir),
            DirClass::Denied(reason) => add_kept(plan, KeptKind::State, &dir, reason.to_string()),
            DirClass::Shared => add_kept(
                plan,
                KeptKind::State,
                &dir,
                "shared directory, not named for madmail; remove it yourself".into(),
            ),
        }
    }
}

fn add_kept(plan: &mut UninstallPlan, kind: KeptKind, path: &Path, reason: String) {
    let entry = Kept {
        kind,
        path: path.to_path_buf(),
        reason,
    };
    if !plan
        .kept
        .iter()
        .any(|k| k.kind == kind && k.path == entry.path)
    {
        plan.kept.push(entry);
    }
}

/// Whether removing `dir` as a whole would also remove `path`.
fn covers(dir: &Path, path: &Path) -> bool {
    normalize(path).starts_with(normalize(dir))
}

/// Make sure no whole-directory removal takes a path that must stay: a kept
/// path, the state directories under `--keep-data`, or the config directories
/// and files under `--keep-config`. A config directory that would take one is
/// downgraded to removing only its config file; a state, log, cache or runtime
/// directory is kept. Repeats until nothing changes, since each downgrade adds
/// a kept path.
fn protect_preserved(plan: &mut UninstallPlan, keep_data: bool, keep_config: bool) {
    loop {
        let mut preserved: Vec<(PathBuf, &'static str)> = plan
            .kept
            .iter()
            .map(|k| (k.path.clone(), "which is left in place"))
            .collect();
        if keep_data {
            for dir in plan
                .state_dirs
                .iter()
                .chain(&plan.log_dirs)
                .chain(&plan.cache_dirs)
                .chain(&plan.runtime_dirs)
            {
                preserved.push((dir.clone(), "which --keep-data keeps"));
            }
        }
        if keep_config {
            for path in plan.config_dirs.iter().chain(&plan.config_files) {
                preserved.push((path.clone(), "which --keep-config keeps"));
            }
        }
        let blocker = |dir: &Path| {
            preserved
                .iter()
                .find(|(p, _)| covers(dir, p))
                .map(|(p, why)| format!("contains {}, {why}", p.display()))
        };

        let mut downgraded: Vec<(KeptKind, PathBuf, String)> = Vec::new();
        if !keep_config {
            for dir in &plan.config_dirs {
                if let Some(why) = blocker(dir) {
                    downgraded.push((KeptKind::Config, dir.clone(), why));
                }
            }
        }
        if !keep_data {
            for dir in plan
                .state_dirs
                .iter()
                .chain(&plan.log_dirs)
                .chain(&plan.cache_dirs)
                .chain(&plan.runtime_dirs)
            {
                if let Some(why) = blocker(dir) {
                    downgraded.push((KeptKind::State, dir.clone(), why));
                }
            }
        }
        if downgraded.is_empty() {
            break;
        }
        for (kind, dir, why) in downgraded {
            match kind {
                KeptKind::Config => {
                    plan.config_dirs.retain(|d| d != &dir);
                    let files: Vec<PathBuf> = plan
                        .config_dir_files
                        .iter()
                        .filter(|(d, _)| d == &dir)
                        .map(|(_, f)| f.clone())
                        .collect();
                    let reason = match files.first() {
                        Some(file) => {
                            push_unique_path(&mut plan.config_files, file.clone());
                            let name = file.file_name().unwrap_or_default().to_string_lossy();
                            format!("{why}; only {name} is removed")
                        }
                        None => format!("{why}; nothing in it is removed"),
                    };
                    add_kept(plan, KeptKind::Config, &dir, reason);
                }
                KeptKind::State => {
                    for list in [
                        &mut plan.state_dirs,
                        &mut plan.log_dirs,
                        &mut plan.cache_dirs,
                        &mut plan.runtime_dirs,
                    ] {
                        list.retain(|d| d != &dir);
                    }
                    add_kept(
                        plan,
                        KeptKind::State,
                        &dir,
                        format!("{why}; remove the rest yourself"),
                    );
                }
            }
        }
    }
    // A file inside a directory that is removed as a whole needs no own entry.
    let dirs = plan.config_dirs.clone();
    plan.config_files
        .retain(|f| !dirs.iter().any(|d| covers(d, f)));
}

/// Split an `ExecStart=` value the way install writes it: whitespace-separated
/// words, double quotes with `\\` / `\"` escapes, and `%%` / `$$` escaping.
fn split_exec_words(exec: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut chars = exec.trim().chars().peekable();
    while chars.peek().is_some() {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut word = String::new();
        let mut started = false;
        let mut quoted = false;
        while let Some(c) = chars.next() {
            match c {
                '"' => {
                    quoted = !quoted;
                    started = true;
                }
                '\\' if quoted => {
                    if let Some(next) = chars.next() {
                        word.push(next);
                    }
                }
                c if c.is_whitespace() && !quoted => break,
                c => {
                    word.push(c);
                    started = true;
                }
            }
        }
        if started || !word.is_empty() {
            words.push(word.replace("%%", "%").replace("$$", "$"));
        }
    }
    words
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
        if is_family_stem(name) || name.starts_with("madmail") || name.starts_with("chatmail") {
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
    if !flags.keep_data {
        for sd in &plan.state_dirs {
            println!("State dir (removed with all contents): {}", sd.display());
        }
    }
    if !flags.keep_config {
        for cd in &plan.config_dirs {
            println!("Config dir (removed with all contents): {}", cd.display());
        }
        for cf in &plan.config_files {
            println!("Config file (removed): {}", cf.display());
        }
    }
    for kept in kept_paths(plan, flags) {
        println!("Left in place: {} ({})", kept.path.display(), kept.reason);
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
    for file in &plan.config_files {
        remove_path(file, flags.dry_run)?;
    }
    Ok(())
}

/// Kept paths that matter for the removal steps that run.
fn kept_paths<'a>(plan: &'a UninstallPlan, flags: &UninstallArgs) -> Vec<&'a Kept> {
    plan.kept
        .iter()
        .filter(|k| match k.kind {
            KeptKind::Config => !flags.keep_config,
            KeptKind::State => !flags.keep_data,
        })
        .collect()
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
        // No `-r`: the home is the state directory, which is removed (or kept
        // with --keep-data, or left as a shared directory) by the data step.
        let _ = Command::new("userdel").arg(user).status();
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
    remove_entry(path)
        .map_err(|e| ChatmailError::config(format!("failed to remove {}: {e}", path.display())))?;
    println!("Removed: {}", path.display());
    Ok(())
}

/// Remove `path` (a directory with all its contents, or a file or link) after
/// checking every directory on the way to it: each must be owned by root and
/// not writable by another user or group (when uninstall is not run as root,
/// the caller's own directories count as well). The last component is looked
/// at through the opened parent and never followed: a link is removed itself,
/// a file is unlinked through the parent descriptor, and a directory is
/// removed with [`fs::remove_dir_all`], which does not follow links inside it.
/// A missing path is not an error.
#[cfg(unix)]
fn remove_entry(path: &Path) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    use std::os::fd::AsRawFd;

    use super::secure_fs::{self, Resolved, Resolver, Trust, Want};

    let path = normalize(path);
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(Error::new(ErrorKind::InvalidInput, "refusing to remove /"));
    };
    if !path.is_absolute() {
        return Err(Error::new(ErrorKind::InvalidInput, "not an absolute path"));
    }
    let trust = Trust::root_or_caller();
    let resolver = Resolver {
        trust: &trust,
        state_root: None,
        check_final: true,
    };
    let dir = match resolver.resolve(parent, Want::Dir) {
        Ok(Resolved::Found(dir)) => dir,
        Ok(Resolved::Missing { .. }) => return Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Ok(Resolved::Controlled { component, reason }) => {
            return Err(Error::other(format!(
                "{} is {reason}; check it and remove the path yourself",
                component.display()
            )))
        }
        Ok(Resolved::InTree(_)) => unreachable!("no state root is set"),
        Err(e) => return Err(e),
    };
    let cname = secure_fs::cstring(name)?;
    let st = match secure_fs::fstatat_nofollow(dir.fd.as_raw_fd(), &cname) {
        Ok(st) => st,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if secure_fs::kind(&st) == libc::S_IFDIR {
        // The parent passed the check, so nobody but root can replace this
        // entry between the check and the removal.
        fs::remove_dir_all(dir.path.join(name))
    } else {
        secure_fs::unlinkat(dir.fd.as_raw_fd(), &cname, false)
    }
}

#[cfg(not(unix))]
fn remove_entry(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    }
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
    fn is_family_stem_matches_madmail_and_chatmail_variants() {
        for stem in [
            "madmail",
            "chatmail",
            "madmail-new",
            "madmail_backup",
            "chatmail-dev",
            "chatmail_staging",
        ] {
            assert!(is_family_stem(stem), "expected family stem: {stem}");
        }
        for stem in ["nginx", "madmailx", "postfix", "other-service"] {
            assert!(!is_family_stem(stem), "unexpected family stem: {stem}");
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
        assert!(plan.state_candidates.iter().any(|p| p == dir.path()));
        assert!(plan.binary_paths.iter().any(|p| p == &binary));
    }

    #[test]
    fn enrich_from_unit_file_finds_custom_config_dir_and_quoted_binary() {
        let dir = tempfile::tempdir().unwrap();
        let bin_dir = dir.path().join("bin dir");
        fs::create_dir(&bin_dir).unwrap();
        let binary = bin_dir.join("madmail");
        fs::write(&binary, b"").unwrap();
        let config_dir = dir.path().join("etc").join("mad%mail");
        let state_dir = dir.path().join("lib").join("mail 100%");
        let unit = dir.path().join("madmail.service");
        let unit_body = format!(
            "[Service]\nWorkingDirectory={state}\nExecStart=\"{bin}\" --config {cfg}/madmail.conf run --libexec \"{state}\"\n",
            state = state_dir.display().to_string().replace('%', "%%"),
            bin = binary.display(),
            cfg = config_dir.display().to_string().replace('%', "%%"),
        );
        fs::write(&unit, unit_body).unwrap();

        let mut plan = UninstallPlan::default();
        enrich_from_unit_file(&mut plan, &unit);

        assert_eq!(plan.binary_paths, vec![binary]);
        assert_eq!(
            plan.config_candidates,
            vec![(config_dir.join("madmail.conf"), Some("madmail".into()))]
        );
        assert_eq!(plan.state_candidates, vec![state_dir]);
    }

    fn unit_plan(configs: &[&Path], state_dirs: &[&Path]) -> UninstallPlan {
        UninstallPlan {
            primary_binary_name: "madmail".into(),
            config_candidates: configs
                .iter()
                .map(|c| (c.to_path_buf(), Some("madmail".into())))
                .collect(),
            state_candidates: state_dirs.iter().map(|d| d.to_path_buf()).collect(),
            ..Default::default()
        }
    }

    fn plan_with(plan: &mut UninstallPlan, keep_data: bool, keep_config: bool) {
        plan_paths(plan, &DenyList::default());
        protect_preserved(plan, keep_data, keep_config);
    }

    fn kept_paths_of(plan: &UninstallPlan) -> Vec<PathBuf> {
        plan.kept.iter().map(|k| k.path.clone()).collect()
    }

    #[test]
    fn dedicated_unit_dirs_are_removed_whole() {
        let base = tempfile::tempdir().unwrap();
        let config_dir = base.path().join("etc/madmail");
        let state_dir = base.path().join("lib/madmail-dev");
        fs::create_dir_all(config_dir.join("certs")).unwrap();
        fs::write(config_dir.join("madmail.conf"), "x").unwrap();
        fs::write(config_dir.join("certs/privkey.pem"), "x").unwrap();
        fs::create_dir_all(state_dir.join("messages")).unwrap();

        let mut plan = unit_plan(&[&config_dir.join("madmail.conf")], &[&state_dir]);
        plan_with(&mut plan, false, false);
        assert_eq!(plan.config_dirs, vec![config_dir.clone()]);
        assert!(plan.config_files.is_empty());
        assert_eq!(plan.state_dirs, vec![state_dir.clone()]);
        assert!(plan.kept.is_empty(), "{:?}", plan.kept);

        let flags = sample_flags(false, base.path().join("log"));
        remove_config(&plan, &flags).unwrap();
        remove_data(&plan, &flags).unwrap();
        assert!(!config_dir.exists() && !state_dir.exists());
    }

    #[test]
    fn shared_config_dir_loses_only_the_config_file() {
        let base = tempfile::tempdir().unwrap();
        let shared = base.path().join("usr-local-etc");
        fs::create_dir_all(shared.join("certs")).unwrap();
        for file in ["madmail.conf", "other.conf", "certs/fullchain.pem"] {
            fs::write(shared.join(file), "x").unwrap();
        }

        let mut plan = unit_plan(&[&shared.join("madmail.conf")], &[]);
        plan_with(&mut plan, false, false);
        assert!(plan.config_dirs.is_empty());
        assert_eq!(plan.config_files, vec![shared.join("madmail.conf")]);
        assert_eq!(
            kept_paths_of(&plan),
            vec![shared.clone(), shared.join("certs/fullchain.pem")]
        );
        assert!(plan.kept[0].reason.contains("only madmail.conf is removed"));

        remove_config(&plan, &sample_flags(false, base.path().join("log"))).unwrap();
        assert!(!shared.join("madmail.conf").exists());
        assert!(shared.join("other.conf").is_file());
        assert!(shared.join("certs/fullchain.pem").is_file());

        // A --config file install would not have written is not removed either.
        let mut plan = unit_plan(&[&shared.join("other.conf")], &[]);
        plan_with(&mut plan, false, false);
        assert!(plan.config_files.is_empty() && plan.config_dirs.is_empty());
        assert_eq!(kept_paths_of(&plan)[0], shared.join("other.conf"));
    }

    #[test]
    fn well_known_and_home_dirs_are_never_removed_whole() {
        let deny = DenyList::new([PathBuf::from("/srv/people/alice")]);
        for dir in [
            "/",
            "/etc",
            "/usr",
            "/usr/local",
            "/usr/local/etc/",
            "/opt",
            "/srv",
            "/var",
            "/var/lib",
            "/var/lib/../lib",
            "/home",
            "/home/admin",
            "/root",
            "/tmp",
            "/srv/people/alice",
        ] {
            assert!(
                matches!(
                    classify_dir(Path::new(dir), "madmail", &deny),
                    DirClass::Denied(_)
                ),
                "{dir}"
            );
        }
        assert_eq!(
            classify_dir(Path::new("/opt/etc/madmail-x"), "madmail", &deny),
            DirClass::Dedicated
        );
        assert_eq!(
            classify_dir(Path::new("/opt/etc"), "madmail", &deny),
            DirClass::Shared
        );

        // `/etc/madmail.conf`: the file goes, `/etc` stays. A denied state
        // directory is kept whatever found it.
        let mut plan = unit_plan(
            &[Path::new("/etc/madmail.conf")],
            &[Path::new("/home/admin"), Path::new("/var/lib")],
        );
        plan_paths(&mut plan, &deny);
        protect_preserved(&mut plan, false, false);
        assert!(plan.config_dirs.is_empty() && plan.state_dirs.is_empty());
        assert_eq!(plan.config_files, vec![PathBuf::from("/etc/madmail.conf")]);
        let kept = kept_paths_of(&plan);
        for path in ["/etc", "/home/admin", "/var/lib"] {
            assert!(kept.contains(&PathBuf::from(path)), "{path}: {kept:?}");
        }
    }

    #[test]
    fn real_user_homes_come_from_passwd() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\n\
                      daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin\n\
                      nobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin\n\
                      alice:x:1000:1000:Alice:/data/alice:/bin/bash\n\
                      madmail:x:1001:1001::/var/lib/madmail:/sbin/nologin\n";
        assert_eq!(
            real_user_homes(passwd, &["madmail".into()]),
            vec![PathBuf::from("/root"), PathBuf::from("/data/alice")]
        );
    }

    #[test]
    fn shared_state_dir_is_kept_whatever_found_it() {
        let base = tempfile::tempdir().unwrap();
        let shared = base.path().join("srv-data");
        fs::create_dir_all(&shared).unwrap();

        let mut plan = unit_plan(&[], &[&shared]);
        plan_with(&mut plan, false, false);
        assert!(plan.state_dirs.is_empty());
        assert_eq!(kept_paths_of(&plan), vec![shared.clone()]);
        assert_eq!(plan.kept[0].kind, KeptKind::State);
        assert_eq!(
            plan.kept[0].reason,
            "shared directory, not named for madmail; remove it yourself"
        );
        // The kept note is only shown when the data step runs.
        let mut flags = sample_flags(true, base.path().join("log"));
        assert_eq!(kept_paths(&plan, &flags).len(), 1);
        flags.keep_data = true;
        assert!(kept_paths(&plan, &flags).is_empty());

        // `--state-dir` is not an exception.
        let flags = sample_flags(true, base.path().join("log"));
        let plan = detect_installation(&shared, None, "madmail-test-only", &flags).unwrap();
        assert!(plan.installation_found);
        assert!(!plan.state_dirs.contains(&shared), "{:?}", plan.state_dirs);
        assert!(kept_paths_of(&plan).contains(&shared));
    }

    #[test]
    fn config_state_dir_naming_a_shared_dir_is_kept() {
        let base = tempfile::tempdir().unwrap();
        let shared = base.path().join("srv-data");
        fs::create_dir_all(shared.join("other")).unwrap();
        let config = base.path().join("madmail.conf");
        fs::write(&config, format!("state_dir {}\n", shared.display())).unwrap();
        let args = Args::try_parse_from([
            "madmail",
            "--config",
            config.to_str().unwrap(),
            "--state-dir",
            base.path().join("cli-state").to_str().unwrap(),
        ])
        .unwrap();
        let ctx = CtlContext::from_args(&args).unwrap();
        let (state_dir, untrusted) = uninstall_state_dir(&args, &ctx);
        assert_eq!(state_dir, shared);
        assert!(untrusted.is_none());

        let flags = sample_flags(false, base.path().join("log"));
        let plan = detect_installation(&state_dir, untrusted, "madmail-test-only", &flags).unwrap();
        assert!(!plan.state_dirs.contains(&shared));
        assert!(kept_paths_of(&plan).contains(&shared));
        remove_data(&plan, &flags).unwrap();
        assert!(shared.join("other").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn config_state_dir_from_an_untrusted_config_is_not_used() {
        use std::os::unix::fs::PermissionsExt;

        let base = tempfile::tempdir().unwrap();
        let open_dir = base.path().join("open");
        fs::create_dir(&open_dir).unwrap();
        fs::set_permissions(&open_dir, fs::Permissions::from_mode(0o777)).unwrap();
        let config = open_dir.join("madmail.conf");
        fs::write(&config, "state_dir /var/lib/madmail-elsewhere\n").unwrap();
        let cli = base.path().join("madmail");
        let args = Args::try_parse_from([
            "madmail",
            "--config",
            config.to_str().unwrap(),
            "--state-dir",
            cli.to_str().unwrap(),
        ])
        .unwrap();
        let ctx = CtlContext::from_args(&args).unwrap();
        let (state_dir, untrusted) = uninstall_state_dir(&args, &ctx);
        assert_eq!(state_dir, cli);
        let kept = untrusted.unwrap();
        assert_eq!(kept.path, PathBuf::from("/var/lib/madmail-elsewhere"));
        assert!(
            kept.reason
                .contains("world-writable without the sticky bit"),
            "{}",
            kept.reason
        );
    }

    #[test]
    fn keep_data_wins_over_a_config_dir_that_contains_the_state_dir() {
        let base = tempfile::tempdir().unwrap();
        let config_dir = base.path().join("madmail");
        let state_dir = config_dir.join("madmail-data");
        fs::create_dir_all(state_dir.join("messages")).unwrap();
        fs::write(config_dir.join("madmail.conf"), "x").unwrap();

        // Also the case where both are the same directory.
        for state in [state_dir.clone(), config_dir.clone()] {
            let mut plan = unit_plan(&[&config_dir.join("madmail.conf")], &[&state]);
            plan_with(&mut plan, true, false);
            assert!(plan.config_dirs.is_empty(), "{:?}", plan.config_dirs);
            assert_eq!(plan.config_files, vec![config_dir.join("madmail.conf")]);
            let kept = plan.kept.iter().find(|k| k.path == config_dir).unwrap();
            assert_eq!(
                kept.reason,
                format!(
                    "contains {}, which --keep-data keeps; only madmail.conf is removed",
                    state.display()
                )
            );
        }

        let mut plan = unit_plan(&[&config_dir.join("madmail.conf")], &[&state_dir]);
        plan_with(&mut plan, true, false);
        let mut flags = sample_flags(false, base.path().join("log"));
        flags.keep_data = true;
        remove_config(&plan, &flags).unwrap();
        assert!(!config_dir.join("madmail.conf").exists());
        assert!(state_dir.join("messages").is_dir());
    }

    #[test]
    fn keep_config_wins_over_a_state_dir_that_contains_the_config() {
        let base = tempfile::tempdir().unwrap();
        let state_dir = base.path().join("madmail");
        let config_dir = state_dir.join("madmail-etc");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("madmail.conf"), "x").unwrap();

        let mut plan = unit_plan(&[&config_dir.join("madmail.conf")], &[&state_dir]);
        plan_with(&mut plan, false, true);
        assert!(plan.state_dirs.is_empty(), "{:?}", plan.state_dirs);
        assert_eq!(plan.config_dirs, vec![config_dir.clone()]);
        let kept = plan.kept.iter().find(|k| k.path == state_dir).unwrap();
        assert_eq!(kept.kind, KeptKind::State);
        assert_eq!(
            kept.reason,
            format!(
                "contains {}, which --keep-config keeps; remove the rest yourself",
                config_dir.display()
            )
        );
        let mut flags = sample_flags(false, base.path().join("log"));
        flags.keep_config = true;
        remove_data(&plan, &flags).unwrap();
        assert!(config_dir.join("madmail.conf").is_file());

        // Without the flags both go.
        let mut plan = unit_plan(&[&config_dir.join("madmail.conf")], &[&state_dir]);
        plan_with(&mut plan, false, false);
        assert_eq!(plan.state_dirs, vec![state_dir.clone()]);
        assert!(plan.kept.is_empty(), "{:?}", plan.kept);
    }

    #[cfg(unix)]
    #[test]
    fn removal_checks_the_path_and_does_not_follow_the_last_link() {
        use std::os::unix::fs::PermissionsExt;

        let base = tempfile::tempdir().unwrap();
        let open_dir = base.path().join("open");
        fs::create_dir_all(open_dir.join("madmail")).unwrap();
        fs::set_permissions(&open_dir, fs::Permissions::from_mode(0o777)).unwrap();
        let err = remove_path(&open_dir.join("madmail"), false).unwrap_err();
        assert!(
            err.to_string()
                .contains("world-writable without the sticky bit"),
            "{err}"
        );
        assert!(open_dir.join("madmail").is_dir());

        let target = base.path().join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep"), "x").unwrap();
        let link = base.path().join("madmail-link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        remove_path(&link, false).unwrap();
        assert!(fs::symlink_metadata(&link).is_err());
        assert!(target.join("keep").is_file());

        remove_path(&base.path().join("missing"), false).unwrap();
    }

    #[test]
    fn split_exec_words_reverses_unit_escaping() {
        assert_eq!(
            split_exec_words(r#""/srv/a \"b\"" --config /srv/%%n/$$HOME run """#),
            vec!["/srv/a \"b\"", "--config", "/srv/%n/$HOME", "run", ""]
        );
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
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("madmail");
        fs::create_dir(&dir).unwrap();
        let ctx = ctl_context(&dir);
        let flags = sample_flags(true, base.path().join("log"));
        let plan = detect_installation(&ctx.state_dir, None, "madmail", &flags).unwrap();

        assert!(plan.installation_found);
        assert!(plan.state_dirs.iter().any(|p| p == &ctx.state_dir));
        assert!(plan.service_names.contains(&"madmail".to_string()));
    }

    #[test]
    fn detect_installation_not_found_without_state_or_artifacts() {
        let base = tempfile::tempdir().unwrap();
        let missing = base.path().join("missing-state");
        let ctx = ctl_context(&missing);
        let flags = sample_flags(true, base.path().join("log"));
        let plan = detect_installation(&ctx.state_dir, None, "madmail-test-only", &flags).unwrap();

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
