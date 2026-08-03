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

//! `madmail versions` — list/use/prune versioned installs (TDD 24).

use chatmail_config::{Args, VersionsCommand};
use chatmail_types::{ChatmailError, Result};
use serde_json::json;

use crate::upgrade::{preflight_binary_for_version_manager, verify_signature};
use crate::version_manager::{
    self, default_stable_binary_path, install_root, list_installed, merge_local_and_remote,
    resolve_active_version, set_active, version_binary_path, version_dir,
};

use super::output::CtlOut;

pub async fn versions(args: &Args, cmd: &VersionsCommand) -> Result<()> {
    // Filesystem + optional blocking HTTP (remote list).
    let args = args.clone();
    let cmd = cmd.clone();
    tokio::task::spawn_blocking(move || versions_blocking(&args, &cmd))
        .await
        .map_err(|e| ChatmailError::config(format!("versions task failed: {e}")))?
}

fn versions_blocking(args: &Args, cmd: &VersionsCommand) -> Result<()> {
    let root = install_root();
    match cmd {
        VersionsCommand::List { remote } => list_cmd(args, &root, *remote),
        VersionsCommand::Current => current_cmd(args, &root),
        VersionsCommand::Use { version } => use_cmd(args, &root, version),
        VersionsCommand::Prune { keep, yes } => prune_cmd(args, &root, *keep, *yes),
        VersionsCommand::Remove { version, yes } => remove_cmd(args, &root, version, *yes),
        VersionsCommand::Path { version } => path_cmd(args, &root, version.as_deref()),
    }
}

fn list_cmd(args: &Args, root: &std::path::Path, remote: bool) -> Result<()> {
    let out = CtlOut::from_args(args, "versions list");
    let local = list_installed(root)?;
    if !remote {
        if out.is_json() {
            return out.emit(json!({
                "install_root": root,
                "versions": local,
            }));
        }
        out.blank();
        out.line(format!("Install root: {}", root.display()));
        if local.is_empty() {
            out.line("  (no versions installed)");
        } else {
            out.line(format!("{:<12} {:<8} {}", "VERSION", "ACTIVE", "PATH"));
            for v in &local {
                out.line(format!(
                    "{:<12} {:<8} {}",
                    v.version,
                    if v.active { "*" } else { "" },
                    v.binary.display()
                ));
            }
        }
        out.blank();
        return Ok(());
    }

    // --remote: metadata only (no binary download / no signature)
    let remote_list = version_manager::fetch_remote_releases(false)
        .map_err(|e| ChatmailError::config(format!("versions list --remote failed: {e}")))?;
    let merged = merge_local_and_remote(&local, &remote_list);
    if out.is_json() {
        return out.emit(json!({
            "install_root": root,
            "remote": true,
            "versions": merged,
        }));
    }
    out.blank();
    out.line(format!("Install root: {}", root.display()));
    out.line(format!(
        "{:<12} {:<8} {:<8} {}",
        "VERSION", "SOURCE", "ACTIVE", "NOTES"
    ));
    for e in &merged {
        let mut notes = Vec::new();
        if e.remote_latest {
            notes.push("remote latest");
        }
        if e.installed {
            notes.push("installed");
        } else {
            notes.push("available");
        }
        if e.source == "local" {
            notes.push("local-only");
        }
        out.line(format!(
            "{:<12} {:<8} {:<8} {}",
            e.version,
            e.source,
            if e.active { "*" } else { "" },
            notes.join("; ")
        ));
    }
    out.blank();
    Ok(())
}

fn current_cmd(args: &Args, root: &std::path::Path) -> Result<()> {
    let out = CtlOut::from_args(args, "versions current");
    let active = resolve_active_version(root)?;
    let path = active.as_ref().map(|id| version_binary_path(root, id));
    if out.is_json() {
        return out.emit(json!({
            "install_root": root,
            "version": active,
            "path": path,
        }));
    }
    match (&active, &path) {
        (Some(v), Some(p)) => {
            out.line(format!("Active version: {v}"));
            out.line(format!("Path: {}", p.display()));
        }
        _ => out.line("No active version under the install root."),
    }
    Ok(())
}

fn use_cmd(args: &Args, root: &std::path::Path, version: &str) -> Result<()> {
    let out = CtlOut::from_args(args, "versions use");
    let id = version_manager::sanitize_version_id(version)?;
    let bin = version_binary_path(root, &id);
    if !bin.is_file() {
        return Err(ChatmailError::config(format!(
            "version {id} not found at {}",
            bin.display()
        )));
    }

    // Mandatory signature check before stop/activate
    eprintln!("🔍 Verifying digital signature for version {id}...");
    match verify_signature(&bin)? {
        true => eprintln!("✅ Signature verification successful."),
        false => {
            return Err(ChatmailError::config(
                "INVALID SIGNATURE: cannot activate this version; refusing to switch",
            ));
        }
    }

    preflight_binary_for_version_manager(&bin)?;

    let prev = resolve_active_version(root)?;
    let stable = default_stable_binary_path();

    // Service stop/start when not in unit-test layout (MADMAIL_INSTALL_ROOT alone still
    // attempts services only if not skipped). Skip when root is under temp / non-default
    // and MADMAIL_VERSION_MANAGER_NO_SERVICE is set, or always try best-effort systemctl.
    let manage_services = std::env::var_os("MADMAIL_VERSION_MANAGER_NO_SERVICE").is_none();

    if manage_services {
        stop_services_best_effort();
    }

    if let Err(e) = set_active(root, &id, &stable) {
        if manage_services {
            start_services_best_effort();
        }
        return Err(e);
    }

    if let Err(e) = preflight_binary_for_version_manager(&bin) {
        // restore previous
        if let Some(p) = prev.as_deref() {
            let _ = set_active(root, p, &stable);
        }
        if manage_services {
            start_services_best_effort();
        }
        return Err(ChatmailError::config(format!(
            "smoke check failed after switch; restored previous: {e}"
        )));
    }

    if manage_services {
        start_services_best_effort();
    }

    out.done_msg(
        format!("✅ Active version is now {id}"),
        json!({
            "version": id,
            "previous": prev,
            "path": bin,
            "signature_ok": true,
        }),
        format!("Active version is now {id}"),
    )
}

fn prune_cmd(args: &Args, root: &std::path::Path, keep: Option<usize>, yes: bool) -> Result<()> {
    let out = CtlOut::from_args(args, "versions prune");
    let keep = keep.unwrap_or(version_manager::default_keep());
    if !yes && !args.json {
        // non-interactive require --yes for destructive
        return Err(ChatmailError::config(
            "versions prune requires --yes to delete old versions",
        ));
    }
    let removed = version_manager::prune(root, keep)?;
    out.done_msg(
        format!(
            "✅ Pruned {} version(s) (keep non-active={keep})",
            removed.len()
        ),
        json!({ "removed": removed, "keep": keep }),
        "Pruned old versions",
    )
}

fn remove_cmd(args: &Args, root: &std::path::Path, version: &str, yes: bool) -> Result<()> {
    let out = CtlOut::from_args(args, "versions remove");
    if !yes && !args.json {
        return Err(ChatmailError::config("versions remove requires --yes"));
    }
    version_manager::remove_version(root, version)?;
    out.done_msg(
        format!("✅ Removed version {version}"),
        json!({ "removed": version }),
        format!("Removed version {version}"),
    )
}

fn path_cmd(args: &Args, root: &std::path::Path, version: Option<&str>) -> Result<()> {
    let out = CtlOut::from_args(args, "versions path");
    let path = match version {
        Some(v) => {
            let id = version_manager::sanitize_version_id(v)?;
            version_binary_path(root, &id)
        }
        None => {
            let id = resolve_active_version(root)?.ok_or_else(|| {
                ChatmailError::config("no active version; pass an explicit version")
            })?;
            version_binary_path(root, &id)
        }
    };
    if out.is_json() {
        return out.emit(json!({
            "path": path,
            "install_root": root,
            "version_dir": version.and_then(sanitize_ok).map(|id| version_dir(root, &id)),
        }));
    }
    out.line(path.display().to_string());
    Ok(())
}

fn sanitize_ok(v: &str) -> Option<String> {
    version_manager::sanitize_version_id(v).ok()
}

fn stop_services_best_effort() {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("systemctl")
            .args(["stop", "madmail.service"])
            .status();
        let _ = std::process::Command::new("systemctl")
            .args(["stop", "chatmail.service"])
            .status();
    }
}

fn start_services_best_effort() {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("systemctl")
            .args(["start", "madmail.service"])
            .status();
        let _ = std::process::Command::new("systemctl")
            .args(["start", "chatmail.service"])
            .status();
    }
}
