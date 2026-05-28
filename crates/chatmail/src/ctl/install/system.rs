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

#![cfg(unix)]

use std::fs;
use std::io;
use std::os::unix::fs::{chown, PermissionsExt};
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
    Err(ChatmailError::config(
        "installation to /etc and /var/lib requires root (use sudo)",
    ))
}

pub fn create_service_user(cfg: &InstallConfig, dry_run: bool) -> Result<()> {
    if cfg.skip_user {
        return Ok(());
    }

    let exists = Command::new("id")
        .arg(&cfg.maddy_user)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if exists {
        println!("   User {} already exists — fixing home/group", cfg.maddy_user);
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
    let status = Command::new("useradd")
        .args([
            "-mrU",
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
            cfg.maddy_user, status.code()
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

    let home = cfg.state_dir.to_string_lossy();
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

    let status = Command::new("usermod")
        .args([
            "-d",
            home.as_ref(),
            "-m",
            "-g",
            &cfg.maddy_group,
            "-s",
            "/sbin/nologin",
            &cfg.maddy_user,
        ])
        .status()
        .map_err(|e| ChatmailError::config(format!("usermod: {e}")))?;
    if !status.success() {
        return Err(ChatmailError::config(format!(
            "usermod failed for {} (home must be absolute, e.g. {})",
            cfg.maddy_user, cfg.state_dir.display()
        )));
    }
    Ok(())
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
        if path.is_file() {
            chown(path, Some(uid), Some(gid)).map_err(|e| io_error("chown", path, e))?;
        }
    }
    if let Some(certs) = cfg.cert_path.parent() {
        chown(certs, Some(0), Some(gid)).ok();
    }
    Ok(())
}

pub fn install_binary(cfg: &InstallConfig, dry_run: bool) -> Result<()> {
    if !cfg.system_install {
        println!("   Skipping binary install (non-system paths)");
        return Ok(());
    }

    let current = std::env::current_exe()
        .map_err(|e| ChatmailError::config(format!("current_exe: {e}")))?;

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

fn chown_tree(root: &Path, uid: u32, gid: u32) -> Result<()> {
    chown(root, Some(uid), Some(gid)).map_err(|e| io_error("chown", root, e))?;
    for entry in fs::read_dir(root).map_err(|e| io_error("read_dir", root, e))? {
        let entry = entry.map_err(|e| io_error("read_dir", root, e))?;
        let path = entry.path();
        if path.is_dir() {
            chown_tree(&path, uid, gid)?;
        } else {
            chown(&path, Some(uid), Some(gid)).map_err(|e| io_error("chown", &path, e))?;
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
