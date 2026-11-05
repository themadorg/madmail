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

//! System install steps (Madmail `install.go`: user, binary, permissions).

use std::fs;
use std::io;
use std::os::unix::fs::{chown, lchown, PermissionsExt};
use std::path::Path;
use std::process::Command;

use chatmail_types::{ChatmailError, Result};

use super::config::InstallConfig;

pub fn require_root_for_system_install(cfg: &InstallConfig, dry_run: bool) -> Result<()> {
    if dry_run {
        return Ok(());
    }
    if !cfg.system_install {
        return Ok(());
    }
    if effective_uid_is_root() {
        return Ok(());
    }
    Err(ChatmailError::config(format!(
        "system install requires root (use sudo): installs service user, binary, and systemd unit; config {}, state {}",
        cfg.config_dir.display(),
        cfg.state_dir.display()
    )))
}

/// A unit-only repair must point at an existing executable and service identity.
pub fn validate_service_prerequisites(cfg: &InstallConfig) -> Result<()> {
    let metadata =
        fs::metadata(&cfg.binary_path).map_err(|e| io_error("stat binary", &cfg.binary_path, e))?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(ChatmailError::config(
            "systemd-only requires an existing executable binary",
        ));
    }
    lookup_uid_gid(&cfg.maddy_user)?;
    let group_exists = Command::new("getent")
        .args(["group", &cfg.maddy_group])
        .output()
        .map_err(|e| ChatmailError::config(format!("getent group: {e}")))?
        .status
        .success();
    if !group_exists {
        return Err(ChatmailError::config(
            "service group missing; run install --user-only first",
        ));
    }
    Ok(())
}

pub fn create_service_user(cfg: &InstallConfig, dry_run: bool) -> Result<()> {
    if cfg.skip_user || !cfg.system_install {
        return Ok(());
    }

    if !dry_run {
        if let Some(parent) = cfg.state_dir.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error("mkdir", parent, e))?;
        }
    }

    let exists = Command::new("id")
        .arg(&cfg.maddy_user)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if exists {
        println!(
            "   User {} already exists — fixing home/group",
            cfg.maddy_user
        );
        return ensure_service_account(cfg, dry_run);
    }

    if dry_run {
        println!(
            "   Would create user {} (home {}, nologin)",
            cfg.maddy_user,
            cfg.state_dir.display()
        );
        return Ok(());
    }

    println!("   Creating user {}", cfg.maddy_user);
    // A partially removed installation can retain its group after user deletion.
    let group_exists = Command::new("getent")
        .args(["group", &cfg.maddy_group])
        .output()
        .map_err(|e| ChatmailError::config(format!("getent group: {e}")))?
        .status
        .success();
    let mut command = Command::new("useradd");
    if group_exists {
        command.args(["-g", &cfg.maddy_group]);
    } else {
        command.arg("-U");
    }
    let status = command
        .args([
            "-mr",
            "-s",
            "/sbin/nologin",
            "-d",
            &cfg.state_dir.to_string_lossy(),
            "-c",
            "madmail mail server",
            &cfg.maddy_user,
        ])
        .status()
        .map_err(|e| ChatmailError::config(format!("useradd: {e}")))?;

    if !status.success() {
        return Err(ChatmailError::config(format!(
            "useradd failed for {} (status {:?})",
            cfg.maddy_user,
            status.code()
        )));
    }
    println!("   ✓ User {}", cfg.maddy_user);
    ensure_service_account(cfg, dry_run)
}

/// Align passwd/group with systemd `User=` / `Group=` (fixes 217/USER after a broken install).
fn ensure_service_account(cfg: &InstallConfig, dry_run: bool) -> Result<()> {
    if dry_run {
        return Ok(());
    }

    if !Command::new("getent")
        .args(["group", &cfg.maddy_group])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        println!("   Creating group {}", cfg.maddy_group);
        let status = Command::new("groupadd")
            .arg(&cfg.maddy_group)
            .status()
            .map_err(|e| ChatmailError::config(format!("groupadd: {e}")))?;
        if !status.success() {
            return Err(ChatmailError::config(format!(
                "groupadd failed for {}",
                cfg.maddy_group
            )));
        }
    }

    let passwd = Command::new("getent")
        .args(["passwd", &cfg.maddy_user])
        .output()
        .map_err(|e| ChatmailError::config(format!("getent passwd: {e}")))?;
    let passwd_text = String::from_utf8_lossy(&passwd.stdout);
    let previous_home = passwd_text
        .lines()
        .next()
        .filter(|_| passwd.status.success())
        .and_then(|line| line.split(':').nth(5))
        .filter(|home| !home.is_empty())
        .ok_or_else(|| ChatmailError::config(format!("cannot read home for {}", cfg.maddy_user)))?;

    let args = usermod_args(
        Path::new(previous_home),
        &cfg.state_dir,
        cfg.state_dir.is_dir(),
        &cfg.maddy_group,
        &cfg.maddy_user,
    );
    let moves_home = args.iter().any(|arg| arg == "-m");
    let status = Command::new("usermod")
        .args(&args)
        .status()
        .map_err(|e| ChatmailError::config(format!("usermod: {e}")))?;
    if !status.success() {
        if !moves_home {
            return Err(ChatmailError::config(format!(
                "usermod failed for {} (exit status {:?}) while setting home {}",
                cfg.maddy_user,
                status.code(),
                cfg.state_dir.display()
            )));
        }
        // A failed move can still update passwd. Restore only the account entry:
        // using -m again could move or overwrite data left by the failed command.
        let rollback = Command::new("usermod")
            .args(["-d", previous_home, &cfg.maddy_user])
            .status();
        let recovery = match rollback {
            Ok(status) if status.success() => format!("previous home restored to {previous_home}"),
            result => format!(
                "could not restore previous home {previous_home}: {result:?}; repair the account manually"
            ),
        };
        return Err(ChatmailError::config(format!(
            "usermod failed for {} (exit status {:?}) while moving home to {}; {recovery}",
            cfg.maddy_user,
            status.code(),
            cfg.state_dir.display()
        )));
    }
    Ok(())
}

/// `usermod` arguments aligning an existing account with the install paths.
/// The home is only moved (`-m`) when the state directory does not exist yet:
/// `usermod -m` refuses an existing target, and repair always has one.
fn usermod_args(
    previous_home: &Path,
    state_dir: &Path,
    state_dir_exists: bool,
    group: &str,
    user: &str,
) -> Vec<String> {
    let mut args = Vec::new();
    if previous_home != state_dir {
        args.push("-d".to_string());
        args.push(state_dir.to_string_lossy().into_owned());
        if !state_dir_exists {
            args.push("-m".to_string());
        }
    }
    for arg in ["-g", group, "-s", "/sbin/nologin", user] {
        args.push(arg.to_string());
    }
    args
}

/// TLS material and config must be readable by the service user.
pub fn setup_config_permissions(cfg: &InstallConfig, dry_run: bool) -> Result<()> {
    if !cfg.system_install || cfg.skip_user {
        return Ok(());
    }

    let (uid, gid) = lookup_uid_gid(&cfg.maddy_user)?;

    if dry_run {
        return Ok(());
    }

    if let Some(conf) = cfg.config_path.parent() {
        let _ = fs::create_dir_all(conf);
        chown(conf, Some(0), Some(gid)).ok();
    }
    if cfg.config_path.is_file() {
        chown(&cfg.config_path, Some(0), Some(gid))
            .map_err(|e| io_error("chown", &cfg.config_path, e))?;
        fs::set_permissions(&cfg.config_path, fs::Permissions::from_mode(0o640))
            .map_err(|e| io_error("chmod", &cfg.config_path, e))?;
    }
    for path in [&cfg.cert_path, &cfg.key_path] {
        if state_dir_symlink(&cfg.state_dir, path) {
            println!(
                "   ! Skipping ownership of symlink {} inside the state directory",
                path.display()
            );
        } else if path.is_file() {
            chown(path, Some(uid), Some(gid)).map_err(|e| io_error("chown", path, e))?;
        }
    }
    if let Some(certs) = cfg.cert_path.parent() {
        if !state_dir_symlink(&cfg.state_dir, certs) {
            chown(certs, Some(0), Some(gid)).ok();
        }
    }
    Ok(())
}

/// The service account can plant links in its own state tree, so root must not
/// chown through them. Operator-chosen links elsewhere (e.g. to an ACME client's
/// live directory) keep being followed.
fn state_dir_symlink(state_dir: &Path, path: &Path) -> bool {
    path.starts_with(state_dir)
        && fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

pub fn install_binary(cfg: &InstallConfig, dry_run: bool) -> Result<()> {
    if !cfg.system_install {
        println!("   Skipping binary install (non-system paths)");
        return Ok(());
    }

    let current =
        std::env::current_exe().map_err(|e| ChatmailError::config(format!("current_exe: {e}")))?;

    if dry_run {
        println!(
            "   Would install {} → {}",
            current.display(),
            cfg.binary_path.display()
        );
        return Ok(());
    }

    if let Some(parent) = cfg.binary_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| ChatmailError::config(format!("mkdir {}: {e}", parent.display())))?;
    }

    if fs::canonicalize(&current).ok() == fs::canonicalize(&cfg.binary_path).ok() {
        println!(
            "   Skipping binary install (already running from {})",
            cfg.binary_path.display()
        );
    } else {
        println!(
            "   Installing {} → {}",
            current.display(),
            cfg.binary_path.display()
        );
        let staging = cfg.binary_path.with_extension("new");
        fs::copy(&current, &staging)
            .map_err(|e| ChatmailError::config(format!("copy binary: {e}")))?;
        fs::rename(&staging, &cfg.binary_path)
            .map_err(|e| ChatmailError::config(format!("install binary: {e}")))?;
    }
    fs::set_permissions(&cfg.binary_path, fs::Permissions::from_mode(0o755))
        .map_err(|e| ChatmailError::config(format!("chmod binary: {e}")))?;
    println!("   ✓ Binary installed");
    Ok(())
}

pub fn setup_permissions(cfg: &InstallConfig, dry_run: bool) -> Result<()> {
    if !cfg.system_install || cfg.skip_user {
        return Ok(());
    }

    let (uid, gid) = lookup_uid_gid(&cfg.maddy_user)?;

    if dry_run {
        println!(
            "   Would chown -R {}:{} {}",
            uid,
            gid,
            cfg.state_dir.display()
        );
        return Ok(());
    }

    println!(
        "   Setting ownership {}:{} on {}",
        cfg.maddy_user,
        cfg.maddy_user,
        cfg.state_dir.display()
    );
    chown_tree(&cfg.state_dir, uid, gid)?;
    println!("   ✓ Permissions set");
    Ok(())
}

fn lookup_uid_gid(name: &str) -> Result<(u32, u32)> {
    let out = Command::new("id")
        .arg("-u")
        .arg(name)
        .output()
        .map_err(|e| ChatmailError::config(format!("id -u: {e}")))?;
    if !out.status.success() {
        return Err(ChatmailError::config(format!("user {name} not found")));
    }
    let uid: u32 = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .map_err(|_| ChatmailError::config(format!("invalid uid for {name}")))?;

    let out = Command::new("id")
        .arg("-g")
        .arg(name)
        .output()
        .map_err(|e| ChatmailError::config(format!("id -g: {e}")))?;
    let gid: u32 = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .map_err(|_| ChatmailError::config(format!("invalid gid for {name}")))?;

    Ok((uid, gid))
}

/// Recursively chown the service-writable state tree without following symlinks:
/// the service account can plant links there, so root must never chown through
/// them or descend into linked directories.
fn chown_tree(root: &Path, uid: u32, gid: u32) -> Result<()> {
    let metadata = fs::symlink_metadata(root).map_err(|e| io_error("stat", root, e))?;
    if metadata.file_type().is_symlink() {
        return Err(ChatmailError::config(format!(
            "refusing to change ownership through symlink {}; use the resolved state directory",
            root.display()
        )));
    }
    chown_entries(root, uid, gid)
}

fn chown_entries(dir: &Path, uid: u32, gid: u32) -> Result<()> {
    lchown(dir, Some(uid), Some(gid)).map_err(|e| io_error("chown", dir, e))?;
    for entry in fs::read_dir(dir).map_err(|e| io_error("read_dir", dir, e))? {
        let entry = entry.map_err(|e| io_error("read_dir", dir, e))?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| io_error("stat", &path, e))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            chown_entries(&path, uid, gid)?;
        } else {
            lchown(&path, Some(uid), Some(gid)).map_err(|e| io_error("chown", &path, e))?;
        }
    }
    Ok(())
}

fn io_error(op: &str, path: &Path, e: io::Error) -> ChatmailError {
    ChatmailError::config(format!("{op} {}: {e}", path.display()))
}

fn effective_uid_is_root() -> bool {
    Command::new("id")
        .args(["-u"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim() == "0")
            } else {
                None
            }
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, MetadataExt};

    #[test]
    fn usermod_keeps_matching_home_and_never_moves_into_existing_state() {
        let state = Path::new("/var/xyzzy/lib/madmail");
        let tail = ["-g", "madmail", "-s", "/sbin/nologin", "madmail"];
        assert_eq!(usermod_args(state, state, true, "madmail", "madmail"), tail);
        let mut repoint = vec!["-d", "/var/xyzzy/lib/madmail"];
        repoint.extend(tail);
        assert_eq!(
            usermod_args(
                Path::new("/var/wrong/madmail"),
                state,
                true,
                "madmail",
                "madmail"
            ),
            repoint
        );
        let mut moved = vec!["-d", "/var/xyzzy/lib/madmail", "-m"];
        moved.extend(tail);
        assert_eq!(
            usermod_args(
                Path::new("/var/wrong/madmail"),
                state,
                false,
                "madmail",
                "madmail"
            ),
            moved
        );
    }

    #[test]
    fn chown_tree_never_follows_symlinks() {
        let outside = tempfile::tempdir().unwrap();
        let target_dir = outside.path().join("dir");
        fs::create_dir(&target_dir).unwrap();
        fs::write(target_dir.join("inner"), "x").unwrap();
        let target_file = outside.path().join("file");
        fs::write(&target_file, "x").unwrap();

        let state = tempfile::tempdir().unwrap();
        let root = state.path();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/data"), "x").unwrap();
        symlink(&target_dir, root.join("dir-link")).unwrap();
        symlink(&target_file, root.join("file-link")).unwrap();
        symlink(root.join("missing"), root.join("dangling")).unwrap();

        let owner = |path: &Path| {
            let m = fs::symlink_metadata(path).unwrap();
            (m.uid(), m.gid())
        };
        let before = owner(&target_file);
        // As root, use an otherwise unused owner so a followed link would be visible.
        // Otherwise chown to self: following the dangling link would fail.
        let (uid, gid) = if effective_uid_is_root() {
            (4242, 4242)
        } else {
            owner(root)
        };
        chown_tree(root, uid, gid).unwrap();

        assert_eq!(owner(&root.join("sub/data")), (uid, gid));
        assert_eq!(owner(&target_file), before);
        assert_eq!(owner(&target_dir), before);
        assert_eq!(owner(&target_dir.join("inner")), before);

        let linked_root = outside.path().join("state-link");
        symlink(root, &linked_root).unwrap();
        let err = chown_tree(&linked_root, uid, gid).unwrap_err().to_string();
        assert!(err.contains("symlink"), "{err}");
    }

    #[test]
    fn only_links_inside_the_state_dir_are_treated_as_planted() {
        let state = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("fullchain.pem");
        fs::write(&target, "x").unwrap();
        let inside = state.path().join("fullchain.pem");
        symlink(&target, &inside).unwrap();
        let operator_link = outside.path().join("link.pem");
        symlink(&target, &operator_link).unwrap();

        assert!(state_dir_symlink(state.path(), &inside));
        assert!(!state_dir_symlink(state.path(), &operator_link));
        assert!(!state_dir_symlink(outside.path(), &target));
    }
}
