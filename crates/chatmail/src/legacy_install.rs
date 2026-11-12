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

//! Move a legacy `maddy` command and systemd unit onto the stable `madmail` binary.
//!
//! The v2 updater installs `/usr/local/bin/madmail`. Installations that still invoke
//! `/usr/local/bin/maddy` and `maddy.service` keep executing the old file after a
//! successful upgrade (issue #244). This module rewrites only the executable path.
//! Config, state, accounts, and the rest of the unit stay where they are.
//!
//! On Windows the systemd helpers stay in the crate so the command can report that
//! this switch is for Linux. Their bodies are unused there.

#![cfg_attr(windows, allow(dead_code))]

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use chatmail_types::{ChatmailError, Result};

const COMPAT_NAME: &str = "maddy";
const DROP_IN_NAME: &str = "madmail-exec.conf";
const VERSION_MARK: &str = "madmail-v2";

/// Directories that hold the stable command and systemd units.
#[derive(Clone, Debug)]
pub struct Roots {
    pub bin_dir: PathBuf,
    pub etc_unit_dir: PathBuf,
    pub vendor_unit_dirs: Vec<PathBuf>,
    pub install_root: PathBuf,
}

/// What `upgrade` should do with the service after the new binary is active.
#[derive(Clone, Debug)]
pub struct UpgradeHooks {
    pub units: Vec<String>,
    /// Units whose program path is still the old executable.
    pub legacy_units: Vec<String>,
    pub plan: Option<Plan>,
}

impl UpgradeHooks {
    pub fn changes_units(&self) -> bool {
        self.plan
            .as_ref()
            .is_some_and(|plan| !plan.edits.is_empty())
    }

    pub fn empty() -> Self {
        Self {
            units: Vec::new(),
            legacy_units: Vec::new(),
            plan: None,
        }
    }
}

/// Question shown by `upgrade` and `update` before a leftover `maddy` service is rewritten.
pub fn legacy_switch_prompt(legacy_units: &[String]) -> String {
    if legacy_units.is_empty() {
        return "We found a maddy command that appears to be from an older version. \
                Would you like to update it to the new madmail setup?"
            .to_string();
    }
    let named_maddy = legacy_units.iter().any(|name| {
        name == "maddy" || name == "maddy.service" || name.strip_suffix(".service") == Some("maddy")
    });
    if named_maddy && legacy_units.len() == 1 {
        return "We found a service named maddy that appears to be from an older version. \
                Would you like to update it to the new madmail setup?"
            .to_string();
    }
    let listed = legacy_units.join(", ");
    if named_maddy {
        format!(
            "We found a service named maddy ({listed}) that appears to be from an older version. \
             Would you like to update it to the new madmail setup?"
        )
    } else {
        format!(
            "We found a service ({listed}) that appears to be from an older version. \
             Would you like to update it to the new madmail setup?"
        )
    }
}

/// Drop the legacy rewrite. Units that already use the stable binary stay.
/// A host whose only unit is the old one keeps that unit running.
pub fn decline_legacy_switch(hooks: &mut UpgradeHooks) {
    let legacy: std::collections::BTreeSet<String> = hooks.legacy_units.drain(..).collect();
    hooks.units.retain(|name| !legacy.contains(name));
    hooks.plan = None;
}

/// File edits and command paths to retarget. Does not include the stable entry.
#[derive(Clone, Debug)]
pub struct Plan {
    pub edits: Vec<FileWrite>,
    pub retarget: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct FileWrite {
    pub path: PathBuf,
    pub contents: String,
    /// `true` when `path` did not exist before apply. Rollback deletes it.
    pub created: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Applied {
    files: Vec<SavedFile>,
    links: Vec<SavedLink>,
}

#[derive(Clone, Debug)]
struct SavedFile {
    path: PathBuf,
    previous: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
struct SavedLink {
    path: PathBuf,
    previous: PreviousEntry,
}

#[derive(Clone, Debug)]
enum PreviousEntry {
    Symlink { target: PathBuf },
    File { backup: PathBuf },
}

#[derive(Clone, Debug)]
struct Inspection {
    action: Action,
    units: Vec<String>,
    legacy_units: Vec<String>,
}

#[derive(Clone, Debug)]
enum Action {
    AlreadyCanonical,
    Migrate(Plan),
    Refuse(String),
}

#[derive(Clone, Debug)]
struct FoundUnit {
    name: String,
    argv0: PathBuf,
    config: Option<String>,
    state_dir: Option<String>,
    libexec: Option<String>,
    uses_stable: bool,
    edit: Option<FileWrite>,
    ambiguous: bool,
}

/// Result of asking systemd which program a unit is actually running.
#[derive(Clone, Debug)]
pub enum RunningCheck {
    Matches(PathBuf),
    NotRunning,
    Mismatch(PathBuf),
    Unknown(String),
}

/// JSON-facing result of the one-time `legacy-migrate` command.
#[derive(Clone, Debug)]
pub struct MigrationReport {
    pub changed: bool,
    pub already_canonical: bool,
    pub stable: String,
    pub services: Vec<String>,
    pub retargeted: Vec<String>,
    pub running: Vec<(String, String)>,
    pub backups: Vec<String>,
}

pub fn system_roots() -> Roots {
    Roots {
        bin_dir: PathBuf::from("/usr/local/bin"),
        etc_unit_dir: PathBuf::from("/etc/systemd/system"),
        vendor_unit_dirs: vec![
            PathBuf::from("/lib/systemd/system"),
            PathBuf::from("/usr/lib/systemd/system"),
        ],
        install_root: crate::version_manager::install_root(),
    }
}

/// Read the host layout. A refusal is an error so upgrade stops before it
/// touches the running service.
pub fn plan_for_upgrade(stable: &Path) -> Result<UpgradeHooks> {
    #[cfg(not(unix))]
    {
        let _ = stable;
        return Ok(UpgradeHooks::empty());
    }
    #[cfg(unix)]
    {
        let mut roots = system_roots();
        if let Some(parent) = stable.parent() {
            if parent.is_absolute() {
                roots.bin_dir = parent.to_path_buf();
            }
        }
        let install_root = roots.install_root.clone();
        let inspection = decide(&roots, stable, |path| {
            binary_is_madmail(path, &install_root, stable)
        })?;
        match inspection.action {
            Action::Refuse(message) => Err(ChatmailError::config(message)),
            Action::AlreadyCanonical => Ok(UpgradeHooks {
                units: inspection.units,
                legacy_units: inspection.legacy_units,
                plan: None,
            }),
            Action::Migrate(plan) => Ok(UpgradeHooks {
                units: inspection.units,
                legacy_units: inspection.legacy_units,
                plan: Some(plan),
            }),
        }
    }
}

pub fn apply(plan: &Plan, stable: &Path) -> Result<Applied> {
    #[cfg(not(unix))]
    {
        let _ = (plan, stable);
        return Err(ChatmailError::config(
            "legacy maddy migration is implemented for Linux systemd installs",
        ));
    }
    #[cfg(unix)]
    {
        if fs::symlink_metadata(stable).is_err() {
            return Err(ChatmailError::config(format!(
                "stable madmail binary {} is missing",
                stable.display()
            )));
        }
        let mut applied = Applied::default();
        if let Err(err) = apply_inner(plan, stable, &mut applied) {
            let _ = restore(&applied);
            return Err(err);
        }
        Ok(applied)
    }
}

pub fn restore(applied: &Applied) -> Result<()> {
    #[cfg(not(unix))]
    {
        let _ = applied;
        return Ok(());
    }
    #[cfg(unix)]
    {
        for saved in applied.links.iter().rev() {
            restore_link(saved)?;
        }
        for saved in applied.files.iter().rev() {
            restore_file(saved)?;
        }
        Ok(())
    }
}

pub fn daemon_reload() -> Result<()> {
    let status = Command::new("systemctl")
        .arg("daemon-reload")
        .status()
        .map_err(|err| ChatmailError::config(format!("systemctl daemon-reload: {err}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(ChatmailError::config(
            "systemctl daemon-reload failed (is systemd available?)",
        ))
    }
}

pub fn check_service(unit: &str, expected: &Path) -> RunningCheck {
    let pid = match systemctl_main_pid(unit) {
        Ok(Some(pid)) => pid,
        Ok(None) => return RunningCheck::NotRunning,
        Err(err) => return RunningCheck::Unknown(err),
    };
    match process_exe(pid) {
        Some(exe) if same_executable(&exe, expected) => RunningCheck::Matches(exe),
        Some(exe) => RunningCheck::Mismatch(exe),
        None => RunningCheck::Unknown(format!("could not read /proc/{pid}/exe for {unit}")),
    }
}

pub fn same_executable(running: &Path, expected: &Path) -> bool {
    match (fs::canonicalize(running), fs::canonicalize(expected)) {
        (Ok(left), Ok(right)) => left == right,
        _ => normalize_lexical(running) == normalize_lexical(expected),
    }
}

pub fn parse_main_pid(output: &str) -> Option<u32> {
    let text = output.trim();
    if text.is_empty() || text == "0" {
        return None;
    }
    text.parse::<u32>().ok().filter(|pid| *pid > 0)
}

/// One-time switch for an install the old `maddy` updater already unpacked.
pub fn bootstrap() -> Result<MigrationReport> {
    #[cfg(not(unix))]
    {
        return Err(ChatmailError::config(
            "legacy-migrate switches a systemd service on Linux. This host does not use that layout.",
        ));
    }
    #[cfg(unix)]
    {
        if unsafe { libc::geteuid() } != 0 {
            return Err(ChatmailError::config(
                "legacy-migrate must be run as root (sudo) to update the service and the maddy command",
            ));
        }
        let stable = crate::version_manager::default_stable_binary_path();
        if fs::symlink_metadata(&stable).is_err() {
            return Err(ChatmailError::config(format!(
                "no canonical madmail binary at {}.\n\
                 An older `maddy upgrade` installs that path and leaves maddy.service on the old executable.\n\
                 From the new release archive, run:\n\
                   sudo ./madmail upgrade <signed-file-or-url>\n\
                 Do not run that upgrade through the old maddy command; its updater cannot switch the service.",
                stable.display()
            )));
        }
        let hooks = plan_for_upgrade(&stable)?;
        let expected = fs::canonicalize(&stable).unwrap_or_else(|_| stable.clone());
        let services = hooks.units.clone();
        match &hooks.plan {
            None => bootstrap_already(&stable, &services, &expected),
            Some(plan) => bootstrap_apply(&stable, &services, &expected, plan),
        }
    }
}

#[cfg(unix)]
fn bootstrap_already(
    stable: &Path,
    services: &[String],
    expected: &Path,
) -> Result<MigrationReport> {
    if services.is_empty() {
        return Ok(report(false, true, stable, services, &[], vec![], vec![]));
    }
    let mut running = Vec::new();
    let mut needs_restart = false;
    for unit in services {
        match check_service(unit, expected) {
            RunningCheck::Matches(path) => running.push((unit.clone(), path.display().to_string())),
            RunningCheck::NotRunning => needs_restart = true,
            RunningCheck::Mismatch(_) => needs_restart = true,
            RunningCheck::Unknown(err) => {
                return Err(ChatmailError::config(format!(
                    "the unit files already point at {}, but {unit} could not be checked: {err}",
                    stable.display()
                )));
            }
        }
    }
    if !needs_restart {
        return Ok(report(false, true, stable, services, &[], running, vec![]));
    }
    stop_units(services);
    if !start_units(services) {
        return Err(ChatmailError::config(format!(
            "the maddy command already points at {}, but {} did not start. \
             The unit file was left unchanged.",
            stable.display(),
            services.join(", ")
        )));
    }
    thread::sleep(Duration::from_secs(1));
    let running = require_running(services, expected)?;
    Ok(report(false, true, stable, services, &[], running, vec![]))
}

#[cfg(unix)]
fn bootstrap_apply(
    stable: &Path,
    services: &[String],
    expected: &Path,
    plan: &Plan,
) -> Result<MigrationReport> {
    let retargeted: Vec<String> = plan
        .retarget
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    stop_units(services);
    let applied = match apply(plan, stable) {
        Ok(applied) => applied,
        Err(err) => {
            let _ = start_units(services);
            return Err(err);
        }
    };
    if !plan.edits.is_empty() && daemon_reload().is_err() {
        let _ = restore(&applied);
        let _ = daemon_reload();
        let _ = start_units(services);
        return Err(ChatmailError::config(
            "legacy-migrate rolled back: systemctl daemon-reload failed",
        ));
    }
    if !start_units(services) {
        let _ = restore(&applied);
        if !plan.edits.is_empty() {
            let _ = daemon_reload();
        }
        let _ = start_units(services);
        return Err(ChatmailError::config(format!(
            "legacy-migrate rolled back: {} did not start. The previous executable and unit were restored.",
            services.join(", ")
        )));
    }
    thread::sleep(Duration::from_secs(1));
    match require_running(services, expected) {
        Ok(running) => Ok(report(
            true,
            false,
            stable,
            services,
            &retargeted,
            running,
            backup_paths(&applied),
        )),
        Err(err) => {
            let _ = restore(&applied);
            if !plan.edits.is_empty() {
                let _ = daemon_reload();
            }
            let _ = start_units(services);
            Err(ChatmailError::config(format!(
                "legacy-migrate rolled back: {err} The previous executable and unit were restored."
            )))
        }
    }
}

fn report(
    changed: bool,
    already_canonical: bool,
    stable: &Path,
    services: &[String],
    retargeted: &[String],
    running: Vec<(String, String)>,
    backups: Vec<String>,
) -> MigrationReport {
    MigrationReport {
        changed,
        already_canonical,
        stable: stable.display().to_string(),
        services: services.to_vec(),
        retargeted: retargeted.to_vec(),
        running,
        backups,
    }
}

#[cfg(unix)]
fn backup_paths(applied: &Applied) -> Vec<String> {
    applied
        .links
        .iter()
        .filter_map(|link| match &link.previous {
            PreviousEntry::File { backup } => Some(backup.display().to_string()),
            _ => None,
        })
        .collect()
}

#[cfg(unix)]
fn require_running(services: &[String], expected: &Path) -> Result<Vec<(String, String)>> {
    let mut running = Vec::new();
    for unit in services {
        match check_service(unit, expected) {
            RunningCheck::Matches(path) => running.push((unit.clone(), path.display().to_string())),
            RunningCheck::NotRunning => {
                return Err(ChatmailError::config(format!(
                    "{unit} is not running the binary at {}",
                    expected.display()
                )));
            }
            RunningCheck::Mismatch(path) => {
                return Err(ChatmailError::config(format!(
                    "{unit} is running {} instead of {}",
                    path.display(),
                    expected.display()
                )));
            }
            RunningCheck::Unknown(err) => {
                return Err(ChatmailError::config(format!(
                    "could not confirm the executable for {unit}: {err}"
                )));
            }
        }
    }
    Ok(running)
}

#[cfg(unix)]
fn stop_units(units: &[String]) {
    for unit in units {
        let _ = Command::new("systemctl").args(["stop", unit]).status();
    }
    thread::sleep(Duration::from_secs(1));
}

#[cfg(unix)]
fn start_units(units: &[String]) -> bool {
    units.iter().all(|unit| {
        Command::new("systemctl")
            .args(["start", unit])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

fn decide(roots: &Roots, stable: &Path, identify: impl Fn(&Path) -> bool) -> Result<Inspection> {
    let found = discover_units(roots, stable, &identify)?;
    if let Some(unit) = found.iter().find(|unit| unit.ambiguous) {
        return Ok(Inspection {
            action: Action::Refuse(format!(
                "refusing to migrate: {} has more than one ExecStart. Leave a single command in the unit or its drop-in, then run madmail legacy-migrate again.",
                unit.name
            )),
            units: Vec::new(),
            legacy_units: Vec::new(),
        });
    }

    let mut groups: Vec<(InstallKey, Vec<FoundUnit>)> = Vec::new();
    for unit in found {
        let key = InstallKey {
            config: unit.config.clone(),
            state_dir: unit.state_dir.clone(),
            libexec: unit.libexec.clone(),
        };
        if let Some((_, units)) = groups.iter_mut().find(|(existing, _)| existing == &key) {
            units.push(unit);
        } else {
            groups.push((key, vec![unit]));
        }
    }
    if groups.len() > 1 {
        return Ok(Inspection {
            action: Action::Refuse(conflict_message(&groups)),
            units: Vec::new(),
            legacy_units: Vec::new(),
        });
    }

    let units = groups.pop().map(|(_, units)| units).unwrap_or_default();
    let legacy: Vec<&FoundUnit> = units.iter().filter(|unit| !unit.uses_stable).collect();
    if legacy.len() > 1 {
        let names = legacy
            .iter()
            .map(|unit| unit.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Ok(Inspection {
            action: Action::Refuse(format!(
                "refusing to migrate: {names} all start this install from the old executable. Disable the extra unit and run madmail legacy-migrate again."
            )),
            units: Vec::new(),
            legacy_units: Vec::new(),
        });
    }

    let mut edits = Vec::new();
    let mut retarget = Vec::new();
    for unit in &units {
        if unit.uses_stable {
            continue;
        }
        if let Some(edit) = &unit.edit {
            edits.push(edit.clone());
        }
        push_retarget(&mut retarget, roots, &unit.argv0);
    }
    push_compat_command(&mut retarget, roots, stable, &identify);

    let legacy_units = units
        .iter()
        .filter(|unit| !unit.uses_stable)
        .map(|unit| unit.name.clone())
        .collect();
    let names = units.iter().map(|unit| unit.name.clone()).collect();
    if edits.is_empty() && retarget.is_empty() {
        Ok(Inspection {
            action: Action::AlreadyCanonical,
            units: names,
            legacy_units,
        })
    } else {
        Ok(Inspection {
            action: Action::Migrate(Plan { edits, retarget }),
            units: names,
            legacy_units,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InstallKey {
    config: Option<String>,
    state_dir: Option<String>,
    libexec: Option<String>,
}

fn conflict_message(groups: &[(InstallKey, Vec<FoundUnit>)]) -> String {
    let mut message = String::from(
        "refusing to migrate: more than one Madmail install is exposed as a service.\n",
    );
    for (_, units) in groups {
        for unit in units {
            message.push_str(&format!(
                "  {} executes {} (config {}, state {})\n",
                unit.name,
                unit.argv0.display(),
                unit.config.as_deref().unwrap_or("(default)"),
                unit.libexec
                    .as_deref()
                    .or(unit.state_dir.as_deref())
                    .unwrap_or("(default)")
            ));
        }
    }
    message.push_str(
        "Nothing was changed. Point the services at one config and state directory, or disable the unrelated unit, then run madmail legacy-migrate again.",
    );
    message
}

fn push_retarget(retarget: &mut Vec<PathBuf>, roots: &Roots, path: &Path) {
    if entry_under_versions(path, &roots.install_root) {
        return;
    }
    if retarget.iter().any(|existing| existing == path) {
        return;
    }
    retarget.push(path.to_path_buf());
}

fn push_compat_command(
    retarget: &mut Vec<PathBuf>,
    roots: &Roots,
    stable: &Path,
    identify: &impl Fn(&Path) -> bool,
) {
    let compat = roots.bin_dir.join(COMPAT_NAME);
    if fs::symlink_metadata(&compat).is_err() {
        return;
    }
    if uses_stable_entry(&compat, stable) || entry_under_versions(&compat, &roots.install_root) {
        return;
    }
    if !identify(&compat) {
        return;
    }
    push_retarget(retarget, roots, &compat);
}

fn discover_units(
    roots: &Roots,
    stable: &Path,
    identify: &impl Fn(&Path) -> bool,
) -> Result<Vec<FoundUnit>> {
    let mut fragments = Vec::new();
    for path in service_files(&roots.etc_unit_dir) {
        fragments.push(path);
    }
    for dir in &roots.vendor_unit_dirs {
        for path in service_files(dir) {
            let name = match path.file_name() {
                Some(name) => name,
                None => continue,
            };
            if fragments
                .iter()
                .any(|existing| existing.file_name() == Some(name))
            {
                continue;
            }
            fragments.push(path);
        }
    }

    let mut found = Vec::new();
    let mut seen = Vec::new();
    for fragment in fragments {
        let canon = fs::canonicalize(&fragment).unwrap_or_else(|_| fragment.clone());
        if seen.iter().any(|existing| existing == &canon) {
            continue;
        }
        seen.push(canon);
        if let Some(unit) = inspect_fragment(roots, stable, &fragment, identify)? {
            found.push(unit);
        }
    }
    Ok(found)
}

fn service_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("service") {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_file() || meta.file_type().is_symlink() {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn inspect_fragment(
    roots: &Roots,
    stable: &Path,
    fragment: &Path,
    identify: &impl Fn(&Path) -> bool,
) -> Result<Option<FoundUnit>> {
    let Some(name) = fragment.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    let name = name.to_string();
    let execs = effective_exec_lines(roots, fragment, &name)?;
    if execs.is_empty() {
        return Ok(None);
    }
    let ours: Vec<&ExecLine> = execs
        .iter()
        .filter(|line| worth_identifying(&line.argv0, roots, stable) && identify(&line.argv0))
        .collect();
    if ours.is_empty() {
        return Ok(None);
    }
    // A continued ExecStart in an unrelated unit (cloud-init, for example) is not
    // our layout. Only a continued line that starts one of our programs is refused.
    if let Some(line) = ours.iter().find(|line| line.continued) {
        return Err(ChatmailError::config(format!(
            "refusing to migrate: {} line {} continues with a backslash. Put ExecStart on one line and run madmail legacy-migrate again.",
            line.file.display(),
            line.line_index + 1
        )));
    }
    if execs.len() != 1 || ours.len() != 1 {
        return Ok(Some(FoundUnit {
            name,
            argv0: ours[0].argv0.clone(),
            config: None,
            state_dir: None,
            libexec: None,
            uses_stable: false,
            edit: None,
            ambiguous: true,
        }));
    }
    let line = ours[0];
    let uses_stable = uses_stable_entry(&line.argv0, stable);
    let edit = if uses_stable {
        None
    } else {
        Some(edit_for(roots, &name, line, stable)?)
    };
    Ok(Some(FoundUnit {
        name,
        argv0: line.argv0.clone(),
        config: line.config.clone(),
        state_dir: line.state_dir.clone(),
        libexec: line.libexec.clone(),
        uses_stable,
        edit,
        ambiguous: false,
    }))
}

struct ExecLine {
    file: PathBuf,
    line_index: usize,
    raw_line: String,
    argv0: PathBuf,
    dash: bool,
    continued: bool,
    config: Option<String>,
    state_dir: Option<String>,
    libexec: Option<String>,
}

fn effective_exec_lines(roots: &Roots, fragment: &Path, unit_name: &str) -> Result<Vec<ExecLine>> {
    let mut current = exec_lines_in(fragment)?;
    for dir in dropin_dirs(roots, fragment, unit_name) {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("conf") {
                    files.push(path);
                }
            }
        }
        files.sort();
        for path in files {
            let text = fs::read_to_string(&path)
                .map_err(|err| ChatmailError::config(format!("read {}: {err}", path.display())))?;
            for (index, raw) in text.lines().enumerate() {
                let trimmed = raw.trim();
                if trimmed == "ExecStart=" {
                    current.clear();
                    continue;
                }
                if let Some(line) = parse_exec_line(&path, index, raw)? {
                    current.push(line);
                }
            }
        }
    }
    Ok(current)
}

fn exec_lines_in(path: &Path) -> Result<Vec<ExecLine>> {
    let text = fs::read_to_string(path)
        .map_err(|err| ChatmailError::config(format!("read {}: {err}", path.display())))?;
    let mut lines = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        if let Some(line) = parse_exec_line(path, index, raw)? {
            lines.push(line);
        }
    }
    Ok(lines)
}

fn parse_exec_line(file: &Path, line_index: usize, raw: &str) -> Result<Option<ExecLine>> {
    let Some(value) = raw.trim().strip_prefix("ExecStart=") else {
        return Ok(None);
    };
    if value.trim().is_empty() {
        return Ok(None);
    }
    let continued = value.trim_end().ends_with('\\');
    let logical = if continued {
        value.trim_end().trim_end_matches('\\')
    } else {
        value
    };
    let (token, _tail) = split_first_token(logical.trim_start());
    if token.is_empty() {
        return Ok(None);
    }
    let (argv0, dash) = argv0_from_token(&token);
    let words = exec_words(logical);
    Ok(Some(ExecLine {
        file: file.to_path_buf(),
        line_index,
        raw_line: raw.trim_end().to_string(),
        argv0,
        dash,
        continued,
        config: flag_value(&words, "--config"),
        state_dir: flag_value(&words, "--state-dir"),
        libexec: flag_value(&words, "--libexec"),
    }))
}

fn dropin_dirs(roots: &Roots, fragment: &Path, unit_name: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(parent) = fragment.parent() {
        let beside = parent.join(format!("{unit_name}.d"));
        if beside.is_dir() {
            dirs.push(beside);
        }
    }
    let etc = roots.etc_unit_dir.join(format!("{unit_name}.d"));
    if etc.is_dir() && !dirs.iter().any(|dir| dir == &etc) {
        dirs.push(etc);
    }
    dirs
}

fn edit_for(roots: &Roots, unit_name: &str, line: &ExecLine, stable: &Path) -> Result<FileWrite> {
    let replacement =
        rewrite_exec_start_line(&line.raw_line, &stable.display().to_string(), line.dash)?;
    if editable_in_place(&line.file, &roots.etc_unit_dir) {
        let text = fs::read_to_string(&line.file)
            .map_err(|err| ChatmailError::config(format!("read {}: {err}", line.file.display())))?;
        return Ok(FileWrite {
            path: line.file.clone(),
            contents: replace_line(&text, line.line_index, &replacement),
            created: false,
        });
    }
    let path = roots
        .etc_unit_dir
        .join(format!("{unit_name}.d"))
        .join(DROP_IN_NAME);
    let contents = if path.is_file() {
        let mut existing = fs::read_to_string(&path)
            .map_err(|err| ChatmailError::config(format!("read {}: {err}", path.display())))?;
        if !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str("ExecStart=\n");
        existing.push_str(&replacement);
        existing.push('\n');
        existing
    } else {
        format!("[Service]\nExecStart=\n{replacement}\n")
    };
    let created = !path.exists();
    Ok(FileWrite {
        path,
        contents,
        created,
    })
}

fn editable_in_place(path: &Path, etc: &Path) -> bool {
    if is_symlink(path) {
        return false;
    }
    entry_under(path, etc)
        && fs::metadata(path)
            .map(|meta| meta.is_file())
            .unwrap_or(false)
}

fn worth_identifying(path: &Path, roots: &Roots, stable: &Path) -> bool {
    if normalize_lexical(path) == normalize_lexical(stable) {
        return true;
    }
    if entry_under(path, &roots.bin_dir) || entry_under_versions(path, &roots.install_root) {
        return true;
    }
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("maddy" | "madmail" | "maddy.exe" | "madmail.exe")
    )
}

fn uses_stable_entry(path: &Path, stable: &Path) -> bool {
    if normalize_lexical(path) == normalize_lexical(stable) {
        return true;
    }
    if !is_symlink(path) {
        return false;
    }
    let Ok(target) = fs::read_link(path) else {
        return false;
    };
    let absolute = if target.is_absolute() {
        target
    } else {
        path.parent().unwrap_or(Path::new("/")).join(target)
    };
    normalize_lexical(&absolute) == normalize_lexical(stable)
}

fn entry_under_versions(path: &Path, install_root: &Path) -> bool {
    entry_under(path, &install_root.join("versions"))
}

fn entry_under(path: &Path, dir: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    if parent.as_os_str().is_empty() {
        return false;
    }
    let parent = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let dir = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    parent == dir || parent.starts_with(&dir)
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

pub fn binary_is_madmail(path: &Path, install_root: &Path, stable: &Path) -> bool {
    if fs::symlink_metadata(path).is_err() {
        return false;
    }
    if normalize_lexical(path) == normalize_lexical(stable)
        || entry_under_versions(path, install_root)
    {
        return true;
    }
    if let Ok(canon) = fs::canonicalize(path) {
        if entry_under_versions(&canon, install_root) {
            return true;
        }
    }
    version_output(path).is_some_and(|text| text.contains(VERSION_MARK))
}

fn version_output(path: &Path) -> Option<String> {
    let mut child = Command::new(path)
        .arg("version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let mut text = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = stdout.read_to_string(&mut text);
                }
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut text);
                }
                return Some(text);
            }
            Ok(None) if started.elapsed() > Duration::from_secs(5) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
}

fn systemctl_main_pid(unit: &str) -> std::result::Result<Option<u32>, String> {
    let output = Command::new("systemctl")
        .args(["show", "-p", "MainPID", "--value", unit])
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(if err.trim().is_empty() {
            format!("systemctl show {unit} failed")
        } else {
            err.trim().to_string()
        });
    }
    Ok(parse_main_pid(&String::from_utf8_lossy(&output.stdout)))
}

fn process_exe(pid: u32) -> Option<PathBuf> {
    let link = fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    let text = link.to_string_lossy();
    Some(match text.strip_suffix(" (deleted)") {
        Some(path) => PathBuf::from(path),
        None => link,
    })
}

#[cfg(unix)]
fn apply_inner(plan: &Plan, stable: &Path, applied: &mut Applied) -> Result<()> {
    for edit in &plan.edits {
        apply_file(edit, applied)?;
    }
    for path in &plan.retarget {
        // A unit may name a program that is not installed. Do not create that name.
        if fs::symlink_metadata(path).is_err() || uses_stable_entry(path, stable) {
            continue;
        }
        apply_link(path, stable, applied)?;
    }
    Ok(())
}

#[cfg(unix)]
fn apply_file(edit: &FileWrite, applied: &mut Applied) -> Result<()> {
    let previous =
        if edit.created {
            None
        } else {
            Some(fs::read(&edit.path).map_err(|err| {
                ChatmailError::config(format!("read {}: {err}", edit.path.display()))
            })?)
        };
    if let Some(bytes) = &previous {
        let backup = pre_migrate_path(&edit.path);
        if !backup.exists() {
            write_bytes(&backup, bytes)?;
        }
    }
    write_text(&edit.path, &edit.contents)?;
    applied.files.push(SavedFile {
        path: edit.path.clone(),
        previous,
    });
    Ok(())
}

#[cfg(unix)]
fn apply_link(path: &Path, stable: &Path, applied: &mut Applied) -> Result<()> {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if meta.file_type().is_symlink() {
        let target = fs::read_link(path)
            .map_err(|err| ChatmailError::config(format!("read link {}: {err}", path.display())))?;
        fs::remove_file(path)
            .map_err(|err| ChatmailError::config(format!("unlink {}: {err}", path.display())))?;
        if let Err(err) = symlink_at(stable, path) {
            let _ = std::os::unix::fs::symlink(&target, path);
            return Err(err);
        }
        applied.links.push(SavedLink {
            path: path.to_path_buf(),
            previous: PreviousEntry::Symlink { target },
        });
        return Ok(());
    }
    if !meta.is_file() {
        return Err(ChatmailError::config(format!(
            "refusing to replace {} because it is not a file or a symlink",
            path.display()
        )));
    }
    let backup = unused_pre_migrate(path);
    fs::copy(path, &backup).map_err(|err| {
        ChatmailError::config(format!(
            "backup {} to {}: {err}",
            path.display(),
            backup.display()
        ))
    })?;
    fs::remove_file(path)
        .map_err(|err| ChatmailError::config(format!("remove {}: {err}", path.display())))?;
    if let Err(err) = symlink_at(stable, path) {
        let _ = fs::rename(&backup, path);
        return Err(err);
    }
    applied.links.push(SavedLink {
        path: path.to_path_buf(),
        previous: PreviousEntry::File { backup },
    });
    Ok(())
}

#[cfg(unix)]
fn restore_link(saved: &SavedLink) -> Result<()> {
    if fs::symlink_metadata(&saved.path).is_ok() {
        fs::remove_file(&saved.path).map_err(|err| {
            ChatmailError::config(format!("remove {}: {err}", saved.path.display()))
        })?;
    }
    match &saved.previous {
        PreviousEntry::Symlink { target } => std::os::unix::fs::symlink(target, &saved.path)
            .map_err(|err| {
                ChatmailError::config(format!("restore link {}: {err}", saved.path.display()))
            }),
        PreviousEntry::File { backup } => fs::rename(backup, &saved.path).map_err(|err| {
            ChatmailError::config(format!(
                "restore {} from {}: {err}",
                saved.path.display(),
                backup.display()
            ))
        }),
    }
}

#[cfg(unix)]
fn restore_file(saved: &SavedFile) -> Result<()> {
    match &saved.previous {
        None => {
            if fs::symlink_metadata(&saved.path).is_ok() {
                fs::remove_file(&saved.path).map_err(|err| {
                    ChatmailError::config(format!("remove {}: {err}", saved.path.display()))
                })?;
            }
            Ok(())
        }
        Some(bytes) => write_bytes(&saved.path, bytes),
    }
}

#[cfg(unix)]
fn symlink_at(target: &Path, link: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| ChatmailError::config(format!("mkdir {}: {err}", parent.display())))?;
    }
    std::os::unix::fs::symlink(target, link).map_err(|err| {
        ChatmailError::config(format!(
            "symlink {} -> {}: {err}",
            link.display(),
            target.display()
        ))
    })
}

fn pre_migrate_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.pre-migrate", path.display()))
}

fn unused_pre_migrate(path: &Path) -> PathBuf {
    let first = pre_migrate_path(path);
    if !first.exists() {
        return first;
    }
    PathBuf::from(format!(
        "{}.pre-migrate.{}",
        path.display(),
        std::process::id()
    ))
}

fn write_text(path: &Path, contents: &str) -> Result<()> {
    write_bytes(path, contents.as_bytes())
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| ChatmailError::config(format!("mkdir {}: {err}", parent.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o755));
        }
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("madmail");
    let tmp = path.with_file_name(format!(".{name}.tmp-{}", std::process::id()));
    {
        let mut file = File::create(&tmp)
            .map_err(|err| ChatmailError::config(format!("create {}: {err}", tmp.display())))?;
        file.write_all(bytes)
            .map_err(|err| ChatmailError::config(format!("write {}: {err}", tmp.display())))?;
        file.sync_all()
            .map_err(|err| ChatmailError::config(format!("sync {}: {err}", tmp.display())))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::symlink_metadata(path)
            .ok()
            .map(|meta| meta.permissions())
            .unwrap_or_else(|| fs::Permissions::from_mode(0o644));
        let _ = fs::set_permissions(&tmp, mode);
    }
    fs::rename(&tmp, path).map_err(|err| {
        let _ = fs::remove_file(&tmp);
        ChatmailError::config(format!("replace {}: {err}", path.display()))
    })
}

/// Replace the executable token on one `ExecStart=` line. `tail` keeps every later argument.
pub fn rewrite_exec_start_line(line: &str, new_bin: &str, dash: bool) -> Result<String> {
    let Some(value) = line.trim_end().strip_prefix("ExecStart=") else {
        return Err(ChatmailError::config(format!(
            "not an ExecStart line: {line}"
        )));
    };
    let (_token, tail) = split_first_token(value.trim_start());
    let mut quoted = quote_systemd_arg(new_bin);
    if dash {
        quoted.insert(0, '-');
    }
    Ok(format!("ExecStart={quoted}{tail}"))
}

fn replace_line(text: &str, index: usize, new_line: &str) -> String {
    let mut out = String::new();
    for (current, line) in text.split_inclusive('\n').enumerate() {
        if current == index {
            let newline = if line.ends_with('\n') { "\n" } else { "" };
            out.push_str(new_line);
            out.push_str(newline);
        } else {
            out.push_str(line);
        }
    }
    out
}

fn argv0_from_token(token: &str) -> (PathBuf, bool) {
    let (path, dash) = match token.strip_prefix('-') {
        Some(rest) if rest.starts_with('/') || rest.starts_with('.') => (rest, true),
        _ => (token, false),
    };
    (PathBuf::from(unescape_systemd(path)), dash)
}

fn exec_words(value: &str) -> Vec<String> {
    let mut rest = value.trim_start();
    let mut words = Vec::new();
    loop {
        let (token, tail) = split_first_token(rest);
        if token.is_empty() {
            break;
        }
        words.push(unescape_systemd(&token));
        rest = tail.trim_start();
    }
    words
}

fn flag_value(words: &[String], flag: &str) -> Option<String> {
    let mut found = None;
    let mut index = 1;
    let with_equals = format!("{flag}=");
    while index < words.len() {
        if let Some(value) = words[index].strip_prefix(&with_equals) {
            found = Some(value.to_string());
            index += 1;
            continue;
        }
        if words[index] == flag {
            if let Some(value) = words.get(index + 1) {
                found = Some(value.clone());
                index += 2;
                continue;
            }
        }
        index += 1;
    }
    found
}

fn split_first_token(input: &str) -> (String, &str) {
    let mut chars = input.char_indices().peekable();
    while matches!(chars.peek(), Some((_, ch)) if ch.is_whitespace()) {
        chars.next();
    }
    let Some((start, first)) = chars.next() else {
        return (String::new(), "");
    };
    if first == '"' {
        let mut token = String::new();
        let mut closed_at = input.len();
        while let Some((index, ch)) = chars.next() {
            if ch == '\\' {
                if let Some((_, next)) = chars.next() {
                    token.push(next);
                }
                continue;
            }
            if ch == '"' {
                closed_at = index + ch.len_utf8();
                break;
            }
            token.push(ch);
        }
        return (token, &input[closed_at..]);
    }
    let mut end = start + first.len_utf8();
    for (index, ch) in chars {
        if ch.is_whitespace() {
            return (input[start..index].to_string(), &input[index..]);
        }
        end = index + ch.len_utf8();
    }
    (input[start..end].to_string(), "")
}

fn unescape_systemd(value: &str) -> String {
    value.replace("%%", "%").replace("$$", "$")
}

fn quote_systemd_arg(arg: &str) -> String {
    let needs = arg.is_empty()
        || arg
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\\' | '\'' | '$' | '%' | ';'));
    if !needs {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    for ch in arg.chars() {
        if ch == '\\' || ch == '"' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_keeps_arguments_and_a_leading_dash() {
        let line = "ExecStart=-/usr/local/bin/maddy --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy";
        let rewritten = rewrite_exec_start_line(line, "/usr/local/bin/madmail", true).unwrap();
        assert_eq!(
            rewritten,
            "ExecStart=-/usr/local/bin/madmail --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy"
        );
    }

    #[test]
    fn rewrite_keeps_a_quoted_argument_tail() {
        let line = "ExecStart=\"/opt/mail bin/maddy\" --config \"/etc/maddy/maddy.conf\" run";
        let rewritten = rewrite_exec_start_line(line, "/usr/local/bin/madmail", false).unwrap();
        assert_eq!(
            rewritten,
            "ExecStart=/usr/local/bin/madmail --config \"/etc/maddy/maddy.conf\" run"
        );
    }

    #[test]
    fn parse_main_pid_rejects_zero_and_blank() {
        assert_eq!(parse_main_pid("123\n"), Some(123));
        assert_eq!(parse_main_pid("0"), None);
        assert_eq!(parse_main_pid(" \n"), None);
    }

    #[test]
    fn same_executable_follows_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("madmail");
        fs::write(&binary, b"bin").unwrap();
        let via = dir.path().join("link");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&binary, &via).unwrap();
            assert!(same_executable(&via, &binary));
        }
        let other = dir.path().join("other");
        fs::write(&other, b"other").unwrap();
        assert!(!same_executable(&other, &binary));
    }

    #[cfg(unix)]
    fn layout() -> (tempfile::TempDir, Roots, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        let etc = dir.path().join("etc/systemd/system");
        let vendor = dir.path().join("lib/systemd/system");
        let root = dir.path().join("opt/madmail");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&etc).unwrap();
        fs::create_dir_all(&vendor).unwrap();
        fs::create_dir_all(root.join("versions/2.26.0")).unwrap();
        let stable = bin.join("madmail");
        fs::write(&stable, b"new").unwrap();
        let roots = Roots {
            bin_dir: bin,
            etc_unit_dir: etc,
            vendor_unit_dirs: vec![vendor],
            install_root: root,
        };
        (dir, roots, stable)
    }

    #[cfg(unix)]
    fn write_unit(path: &Path, exec: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(
            path,
            format!(
                "[Service]\nUser=maddy\nEnvironment=FOO=bar\nExecStart={exec}\nRestart=on-failure\n"
            ),
        )
        .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn regular_maddy_file_and_custom_exec_are_retargeted() {
        let (_dir, roots, stable) = layout();
        let maddy = roots.bin_dir.join("maddy");
        fs::write(&maddy, b"old-binary").unwrap();
        let custom = _dir.path().join("opt/bin/maddy");
        fs::create_dir_all(custom.parent().unwrap()).unwrap();
        fs::write(&custom, b"custom-old").unwrap();
        let unit = roots.etc_unit_dir.join("maddy.service");
        write_unit(
            &unit,
            &format!(
                "{} --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy",
                custom.display()
            ),
        );
        let ours = |path: &Path| path == maddy || path == custom || path == stable;
        let inspection = decide(&roots, &stable, ours).unwrap();
        let plan = match inspection.action {
            Action::Migrate(plan) => plan,
            other => panic!("expected migrate, got {other:?}"),
        };
        assert_eq!(inspection.units, vec!["maddy.service".to_string()]);
        apply(&plan, &stable).unwrap();

        let text = fs::read_to_string(&unit).unwrap();
        assert!(text.contains("User=maddy"));
        assert!(text.contains("Environment=FOO=bar"));
        assert!(text.contains(&format!(
            "ExecStart={} --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy",
            stable.display()
        )));
        assert!(is_symlink(&maddy));
        assert!(uses_stable_entry(&maddy, &stable));
        assert!(is_symlink(&custom));
        assert_eq!(
            fs::read(custom.with_extension("pre-migrate")).unwrap(),
            b"custom-old"
        );
        let again = decide(&roots, &stable, ours).unwrap();
        assert!(matches!(again.action, Action::AlreadyCanonical));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_into_the_version_tree_is_repointed_without_touching_the_archive() {
        let (_dir, roots, stable) = layout();
        let archived = roots.install_root.join("versions/2.26.0/madmail");
        fs::write(&archived, b"archived").unwrap();
        let maddy = roots.bin_dir.join("maddy");
        std::os::unix::fs::symlink(&archived, &maddy).unwrap();
        let unit = roots.etc_unit_dir.join("maddy.service");
        write_unit(
            &unit,
            &format!(
                "{} --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy",
                maddy.display()
            ),
        );
        let plan = match decide(&roots, &stable, |_| true).unwrap().action {
            Action::Migrate(plan) => plan,
            other => panic!("expected migrate, got {other:?}"),
        };
        apply(&plan, &stable).unwrap();
        assert!(uses_stable_entry(&maddy, &stable));
        assert_eq!(fs::read(&archived).unwrap(), b"archived");
        assert!(!is_symlink(&archived));
    }

    #[cfg(unix)]
    #[test]
    fn already_canonical_layout_is_left_alone() {
        let (_dir, roots, stable) = layout();
        let maddy = roots.bin_dir.join("maddy");
        std::os::unix::fs::symlink(&stable, &maddy).unwrap();
        let unit = roots.etc_unit_dir.join("madmail.service");
        write_unit(
            &unit,
            &format!(
                "{} --config /etc/madmail/madmail.conf run --libexec /var/lib/madmail",
                stable.display()
            ),
        );
        let before = fs::read(&unit).unwrap();
        let inspection = decide(&roots, &stable, |_| true).unwrap();
        assert!(matches!(inspection.action, Action::AlreadyCanonical));
        assert_eq!(inspection.units, vec!["madmail.service".to_string()]);
        assert_eq!(fs::read(&unit).unwrap(), before);
    }

    #[cfg(unix)]
    #[test]
    fn conflicting_installs_and_two_legacy_units_are_refused() {
        let (_dir, roots, stable) = layout();
        write_unit(
            &roots.etc_unit_dir.join("maddy.service"),
            "/usr/local/bin/maddy --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy",
        );
        write_unit(
            &roots.etc_unit_dir.join("madmail.service"),
            "/usr/local/bin/madmail --config /etc/madmail/madmail.conf run --libexec /var/lib/madmail",
        );
        let inspection = decide(&roots, &stable, |_| true).unwrap();
        match inspection.action {
            Action::Refuse(message) => {
                assert!(message.contains("more than one Madmail install"));
                assert!(message.contains("/etc/maddy/maddy.conf"));
            }
            other => panic!("expected refuse, got {other:?}"),
        }

        fs::remove_file(roots.etc_unit_dir.join("madmail.service")).unwrap();
        write_unit(
            &roots.etc_unit_dir.join("mail.service"),
            "/opt/old/maddy --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy",
        );
        let inspection = decide(&roots, &stable, |_| true).unwrap();
        match inspection.action {
            Action::Refuse(message) => assert!(message.contains("all start this install")),
            other => panic!("expected refuse, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn one_canonical_unit_beside_one_legacy_unit_migrates_only_the_legacy_unit() {
        let (_dir, roots, stable) = layout();
        let legacy = roots.etc_unit_dir.join("maddy.service");
        let current = roots.etc_unit_dir.join("madmail.service");
        let old = roots.bin_dir.join("maddy");
        fs::write(&old, b"old").unwrap();
        write_unit(
            &legacy,
            &format!(
                "{} --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy",
                old.display()
            ),
        );
        write_unit(
            &current,
            &format!(
                "{} --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy",
                stable.display()
            ),
        );
        let before = fs::read(&current).unwrap();
        let plan = match decide(&roots, &stable, |_| true).unwrap().action {
            Action::Migrate(plan) => plan,
            other => panic!("expected migrate, got {other:?}"),
        };
        assert_eq!(plan.edits.len(), 1);
        apply(&plan, &stable).unwrap();
        assert_eq!(fs::read(&current).unwrap(), before);
        assert!(fs::read_to_string(&legacy)
            .unwrap()
            .contains(&stable.display().to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn drop_in_is_edited_and_a_vendor_unit_gets_an_etc_override() {
        let (_dir, roots, stable) = layout();
        let fragment = roots.etc_unit_dir.join("maddy.service");
        write_unit(
            &fragment,
            "/usr/local/bin/maddy --config /etc/maddy/maddy.conf run",
        );
        let dropin_dir = roots.etc_unit_dir.join("maddy.service.d");
        fs::create_dir_all(&dropin_dir).unwrap();
        fs::write(
            dropin_dir.join("override.conf"),
            format!(
                "[Service]\nExecStart=\nExecStart={} --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy\nEnvironment=KEEP=1\n",
                roots.bin_dir.join("override-maddy").display()
            ),
        )
        .unwrap();
        let plan = match decide(&roots, &stable, |_| true).unwrap().action {
            Action::Migrate(plan) => plan,
            other => panic!("expected migrate, got {other:?}"),
        };
        apply(&plan, &stable).unwrap();
        let fragment_text = fs::read_to_string(&fragment).unwrap();
        assert!(fragment_text.contains("ExecStart=/usr/local/bin/maddy"));
        let dropin = fs::read_to_string(dropin_dir.join("override.conf")).unwrap();
        assert!(dropin.contains("Environment=KEEP=1"));
        assert!(dropin.contains(&format!(
            "ExecStart={} --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy",
            stable.display()
        )));

        fs::remove_dir_all(&roots.etc_unit_dir).unwrap();
        fs::create_dir_all(&roots.etc_unit_dir).unwrap();
        let vendor = roots.vendor_unit_dirs[0].join("maddy.service");
        write_unit(
            &vendor,
            &format!(
                "{} --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy",
                roots.bin_dir.join("vendor-maddy").display()
            ),
        );
        let vendor_before = fs::read(&vendor).unwrap();
        let plan = match decide(&roots, &stable, |_| true).unwrap().action {
            Action::Migrate(plan) => plan,
            other => panic!("expected migrate, got {other:?}"),
        };
        apply(&plan, &stable).unwrap();
        assert_eq!(fs::read(&vendor).unwrap(), vendor_before);
        let override_path = roots
            .etc_unit_dir
            .join("maddy.service.d")
            .join("madmail-exec.conf");
        let override_text = fs::read_to_string(&override_path).unwrap();
        assert!(override_text.contains("ExecStart=\n"));
        assert!(override_text.contains(&stable.display().to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn foreign_maddy_is_ignored_and_rollback_restores_a_regular_file() {
        let (_dir, roots, stable) = layout();
        let foreign = PathBuf::from("/usr/bin/maddy");
        write_unit(
            &roots.etc_unit_dir.join("maddy.service"),
            "/usr/bin/maddy --config /etc/maddymail/maddy.conf run",
        );
        let before = fs::read(roots.etc_unit_dir.join("maddy.service")).unwrap();
        let inspection = decide(&roots, &stable, |path| path != foreign).unwrap();
        assert!(matches!(inspection.action, Action::AlreadyCanonical));
        assert!(inspection.units.is_empty());
        assert_eq!(
            fs::read(roots.etc_unit_dir.join("maddy.service")).unwrap(),
            before
        );

        fs::write(roots.etc_unit_dir.join("maddy.service"), &before).unwrap();
        let maddy = roots.bin_dir.join("maddy");
        fs::write(&maddy, b"old-binary").unwrap();
        write_unit(
            &roots.etc_unit_dir.join("maddy.service"),
            &format!(
                "{} --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy",
                maddy.display()
            ),
        );
        let unit_before = fs::read(roots.etc_unit_dir.join("maddy.service")).unwrap();
        let plan = match decide(&roots, &stable, |_| true).unwrap().action {
            Action::Migrate(plan) => plan,
            other => panic!("expected migrate, got {other:?}"),
        };
        let applied = apply(&plan, &stable).unwrap();
        assert!(is_symlink(&maddy));
        restore(&applied).unwrap();
        assert_eq!(fs::read(&maddy).unwrap(), b"old-binary");
        assert!(!is_symlink(&maddy));
        assert_eq!(
            fs::read(roots.etc_unit_dir.join("maddy.service")).unwrap(),
            unit_before
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_continued_exec_start_in_an_unrelated_unit_is_ignored() {
        let (_dir, roots, stable) = layout();
        let maddy = roots.bin_dir.join("maddy");
        fs::write(&maddy, b"old-binary").unwrap();
        write_unit(
            &roots.etc_unit_dir.join("maddy.service"),
            &format!(
                "{} --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy",
                maddy.display()
            ),
        );
        fs::write(
            roots.vendor_unit_dirs[0].join("cloud-init-hotplugd.service"),
            "[Service]\nExecStart=/bin/bash -c 'echo \\\n    exec /usr/bin/cloud-init'\n",
        )
        .unwrap();
        let inspection = decide(&roots, &stable, |path| path == maddy || path == stable).unwrap();
        match inspection.action {
            Action::Migrate(plan) => {
                assert_eq!(inspection.units, vec!["maddy.service".to_string()]);
                assert_eq!(plan.edits.len(), 1);
            }
            other => panic!("expected migrate, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_continued_exec_start_on_our_unit_is_refused() {
        let (_dir, roots, stable) = layout();
        fs::write(
            roots.etc_unit_dir.join("maddy.service"),
            "[Service]\nExecStart=/usr/local/bin/maddy \\\n    --config /etc/maddy/maddy.conf run\n",
        )
        .unwrap();
        let err = decide(&roots, &stable, |_| true).unwrap_err();
        assert!(
            err.to_string().contains("backslash"),
            "expected backslash refusal, got {err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn multiple_exec_start_lines_are_refused() {
        let (_dir, roots, stable) = layout();
        fs::write(
            roots.etc_unit_dir.join("maddy.service"),
            "[Service]\nExecStart=/bin/maddy one\nExecStart=/bin/maddy two\n",
        )
        .unwrap();
        let inspection = decide(&roots, &stable, |_| true).unwrap();
        match inspection.action {
            Action::Refuse(message) => assert!(message.contains("more than one ExecStart")),
            other => panic!("expected refuse, got {other:?}"),
        }
    }

    #[test]
    fn legacy_switch_prompt_names_the_maddy_service() {
        let prompt = legacy_switch_prompt(&["maddy.service".to_string()]);
        assert_eq!(
            prompt,
            "We found a service named maddy that appears to be from an older version. \
             Would you like to update it to the new madmail setup?"
        );
    }

    #[test]
    fn declining_drops_the_legacy_unit_and_keeps_a_canonical_one() {
        let mut hooks = UpgradeHooks {
            units: vec!["madmail.service".into(), "maddy.service".into()],
            legacy_units: vec!["maddy.service".into()],
            plan: Some(Plan {
                edits: Vec::new(),
                retarget: vec![PathBuf::from("/usr/local/bin/maddy")],
            }),
        };
        decline_legacy_switch(&mut hooks);
        assert!(hooks.plan.is_none());
        assert!(hooks.legacy_units.is_empty());
        assert_eq!(hooks.units, vec!["madmail.service".to_string()]);
    }

    #[test]
    fn declining_the_only_legacy_unit_leaves_nothing_to_restart() {
        let mut hooks = UpgradeHooks {
            units: vec!["maddy.service".into()],
            legacy_units: vec!["maddy.service".into()],
            plan: Some(Plan {
                edits: Vec::new(),
                retarget: Vec::new(),
            }),
        };
        decline_legacy_switch(&mut hooks);
        assert!(hooks.units.is_empty());
        assert!(hooks.plan.is_none());
    }
}
