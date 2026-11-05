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
use std::os::unix::fs::{chown, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use chatmail_types::{ChatmailError, Result};

use super::chown_walk;
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

/// `adopt_existing_state` is set by repair: an existing account is then repointed
/// at the existing state directory. A full install refuses that instead, because
/// data in the previous home would be left behind.
pub fn create_service_user(
    cfg: &InstallConfig,
    dry_run: bool,
    adopt_existing_state: bool,
) -> Result<()> {
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
        return ensure_service_account(cfg, dry_run, adopt_existing_state);
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
    ensure_service_account(cfg, dry_run, adopt_existing_state)
}

/// Align passwd/group with systemd `User=` / `Group=` (fixes 217/USER after a broken install).
fn ensure_service_account(
    cfg: &InstallConfig,
    dry_run: bool,
    adopt_existing_state: bool,
) -> Result<()> {
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
        home_for_comparison(Path::new(previous_home), &cfg.state_dir),
        &cfg.state_dir,
        cfg.state_dir.is_dir(),
        Path::new(previous_home).exists(),
        adopt_existing_state,
        &cfg.maddy_group,
        &cfg.maddy_user,
    )?;
    let moves_home = args.iter().any(|arg| arg == "-m");
    let status = Command::new("usermod")
        .args(&args)
        .status()
        .map_err(|e| ChatmailError::config(format!("usermod: {e}")))?;
    if !status.success() {
        if !moves_home {
            let step = if args.iter().any(|arg| arg == "-d") {
                format!("setting home {}", cfg.state_dir.display())
            } else {
                "updating group/shell".to_string()
            };
            return Err(ChatmailError::config(format!(
                "usermod failed for {} (exit status {:?}) while {step}",
                cfg.maddy_user,
                status.code()
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
/// `usermod -m` refuses an existing target. Repair always has one and adopts it
/// with `-d` alone. A full install adopts it only when the previous home is gone;
/// otherwise it refuses, as it would leave that data behind and continue with a
/// different state directory.
fn usermod_args(
    previous_home: &Path,
    state_dir: &Path,
    state_dir_exists: bool,
    previous_home_exists: bool,
    adopt_existing_state: bool,
    group: &str,
    user: &str,
) -> Result<Vec<String>> {
    let mut args = Vec::new();
    if previous_home != state_dir {
        if state_dir_exists && previous_home_exists && !adopt_existing_state {
            return Err(ChatmailError::config(format!(
                "user {user} has home {} but state directory {} already exists; \
                 a full install would leave the data in {} behind. Move it there if \
                 needed, run `install --user-only` with the same paths to repoint the \
                 account, then retry",
                previous_home.display(),
                state_dir.display(),
                previous_home.display()
            )));
        }
        args.push("-d".to_string());
        args.push(state_dir.to_string_lossy().into_owned());
        if !state_dir_exists {
            args.push("-m".to_string());
        }
    }
    for arg in ["-g", group, "-s", "/sbin/nologin", user] {
        args.push(arg.to_string());
    }
    Ok(args)
}

/// TLS material and config must be readable by the service user.
///
/// Files inside the service-writable state tree are left to [`setup_permissions`]:
/// the service account controls every name there, so a chown by path could be
/// redirected through a link it planted or swapped in; the descriptor-based
/// state walk gives those files `uid:gid` without following links. Files outside
/// the tree (root-owned directories such as `/etc/madmail/certs` or an ACME
/// client's live directory) are changed by path, following links: only root can
/// change those directories, so a link there is the operator's choice.
pub fn setup_config_permissions(cfg: &InstallConfig, dry_run: bool) -> Result<()> {
    if !cfg.system_install || cfg.skip_user {
        return Ok(());
    }

    let (uid, gid) = lookup_uid_gid(&cfg.maddy_user)?;

    if dry_run {
        return Ok(());
    }

    let tree = ServiceTree::new(&cfg.state_dir);
    if let Some(conf) = cfg.config_path.parent() {
        let _ = fs::create_dir_all(conf);
        if !tree.contains(conf) {
            chown(conf, Some(0), Some(gid)).ok();
        }
    }
    if tree.contains(&cfg.config_path) {
        skip_note(&cfg.config_path);
    } else if cfg.config_path.is_file() {
        chown(&cfg.config_path, Some(0), Some(gid))
            .map_err(|e| io_error("chown", &cfg.config_path, e))?;
        fs::set_permissions(&cfg.config_path, fs::Permissions::from_mode(0o640))
            .map_err(|e| io_error("chmod", &cfg.config_path, e))?;
    }
    chown_tls_files(&tree, &cfg.cert_path, &cfg.key_path, uid, gid)?;
    Ok(())
}

/// Chown the cert and key outside the service tree; returns the paths skipped
/// because they are inside it.
fn chown_tls_files(
    tree: &ServiceTree,
    cert: &Path,
    key: &Path,
    uid: u32,
    gid: u32,
) -> Result<Vec<PathBuf>> {
    let mut skipped = Vec::new();
    for path in [cert, key] {
        if tree.contains(path) {
            if fs::symlink_metadata(path).is_ok() {
                skip_note(path);
            }
            skipped.push(path.to_path_buf());
        } else if path.is_file() {
            chown(path, Some(uid), Some(gid)).map_err(|e| io_error("chown", path, e))?;
        }
    }
    if let Some(certs) = cert.parent() {
        if !tree.contains(certs) {
            chown(certs, Some(0), Some(gid)).ok();
        }
    }
    Ok(skipped)
}

fn skip_note(path: &Path) {
    println!(
        "   ! Skipping ownership of {}: inside the service-writable state directory; \
         the state walk sets its owner without following links",
        path.display()
    );
}

/// The service-writable state tree, by its configured and its resolved path.
struct ServiceTree<'a> {
    state_dir: &'a Path,
    canonical: Option<PathBuf>,
}

impl<'a> ServiceTree<'a> {
    fn new(state_dir: &'a Path) -> Self {
        Self {
            state_dir,
            canonical: fs::canonicalize(state_dir).ok(),
        }
    }

    /// A path is in the tree when it is lexically below the state directory, or
    /// when it or any of its ancestors resolves below the resolved state
    /// directory, e.g. through an operator link into the tree. Checking every
    /// ancestor matters: below a component that resolves into the tree, the
    /// service account controls the rest of the path, including a final link
    /// that would make the whole path resolve elsewhere (such as `/etc/shadow`).
    ///
    /// Resolving a whole ancestor only shows where the last hop ends, so each
    /// link on the way is also followed one hop at a time: an operator link to
    /// a file in the tree (`/etc/madmail/certs/fullchain.pem` pointing at
    /// `<state>/certs/fullchain.pem`) is in the tree even after the service
    /// account has replaced that file with a link out of it. At most
    /// [`MAX_LINK_HOPS`] links are followed for one path.
    ///
    /// Resolving follows links, but only to decide that root must not touch the
    /// path here, which is the safe direction.
    fn contains(&self, path: &Path) -> bool {
        let mut hops = MAX_LINK_HOPS;
        self.contains_within(path, &mut hops)
    }

    fn contains_within(&self, path: &Path, hops: &mut usize) -> bool {
        if self.lexically_inside(path) {
            return true;
        }
        for ancestor in path.ancestors() {
            if let Some(canonical) = &self.canonical {
                if fs::canonicalize(ancestor).is_ok_and(|r| r.starts_with(canonical)) {
                    return true;
                }
            }
            let is_link = fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink());
            if !is_link || *hops == 0 {
                continue;
            }
            *hops -= 1;
            let Ok(target) = fs::read_link(ancestor) else {
                continue;
            };
            // A relative target is resolved against the link's directory; the
            // part of `path` below the link follows the target.
            let base = ancestor.parent().unwrap_or(Path::new("/"));
            let mut next = base.join(target);
            if let Ok(rest) = path.strip_prefix(ancestor) {
                if !rest.as_os_str().is_empty() {
                    next.push(rest);
                }
            }
            if self.contains_within(&next, hops) {
                return true;
            }
        }
        false
    }

    /// Lexically below the configured or the resolved state directory, as
    /// written or with `.` and `..` removed.
    fn lexically_inside(&self, path: &Path) -> bool {
        let normalized = lexical_normalize(path);
        [path, normalized.as_path()].iter().any(|p| {
            p.starts_with(self.state_dir)
                || self.canonical.as_ref().is_some_and(|c| p.starts_with(c))
        })
    }
}

/// Links followed one hop at a time by [`ServiceTree::contains`] for one path,
/// as for the kernel's own limit on nested links (`ELOOP`).
const MAX_LINK_HOPS: usize = 40;

/// Remove `.` and resolve `..` lexically (without looking at the file system).
fn lexical_normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// A passwd home that resolves to the state directory is already correct, so it
/// is compared as the state directory itself.
fn home_for_comparison<'a>(previous_home: &'a Path, state_dir: &'a Path) -> &'a Path {
    if same_dir(previous_home, state_dir) {
        state_dir
    } else {
        previous_home
    }
}

/// Whether two paths name the same directory: equal as written, or resolving to
/// the same place (e.g. a symlinked `--state-dir` and the canonical passwd home).
fn same_dir(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (fs::canonicalize(a), fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
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
/// the service account can plant links there and swap entries while root walks,
/// so the walk works on file descriptors (see [`chown_walk`]). The root itself is
/// chosen by the operator (e.g. a link to a data volume), so it is resolved once
/// first; the resolved directory is then opened with `O_NOFOLLOW`, and the walk
/// fails if it was replaced by a link in between.
fn chown_tree(root: &Path, uid: u32, gid: u32) -> Result<()> {
    let root = fs::canonicalize(root).map_err(|e| io_error("resolve", root, e))?;
    if chown_walk::protected_hardlinks_disabled() {
        println!(
            "   ! fs.protected_hardlinks is 0: the service account can hard-link files it \
             does not own into {}, and this walk would hand them over. Set \
             fs.protected_hardlinks=1 (sysctl) and re-run if that may have happened",
            root.display()
        );
    }
    let skipped =
        chown_walk::chown_tree(&root, uid, gid).map_err(|e| io_error(e.op, &e.path, e.source))?;
    for entry in skipped {
        println!(
            "   ! Ownership of {} left unchanged: {}",
            entry.path.display(),
            entry.reason
        );
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
        let old = Path::new("/var/wrong/madmail");
        let tail = ["-g", "madmail", "-s", "/sbin/nologin", "madmail"];
        let args = |previous, state_exists, previous_exists, adopt| {
            usermod_args(
                previous,
                state,
                state_exists,
                previous_exists,
                adopt,
                "madmail",
                "madmail",
            )
        };
        assert_eq!(args(state, true, true, false).unwrap(), tail);
        let mut repoint = vec!["-d", "/var/xyzzy/lib/madmail"];
        repoint.extend(tail);
        // Repair adopts the existing state dir; a full install only does so when
        // nothing is left behind in the previous home.
        assert_eq!(args(old, true, true, true).unwrap(), repoint);
        assert_eq!(args(old, true, false, false).unwrap(), repoint);
        let mut moved = vec!["-d", "/var/xyzzy/lib/madmail", "-m"];
        moved.extend(tail);
        assert_eq!(args(old, false, true, false).unwrap(), moved);
        assert_eq!(args(old, false, true, true).unwrap(), moved);
    }

    #[test]
    fn full_install_refuses_to_abandon_previous_home() {
        let err = usermod_args(
            Path::new("/var/lib/madmail"),
            Path::new("/srv/madmail"),
            true,
            true,
            false,
            "madmail",
            "madmail",
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("/var/lib/madmail")
                && err.contains("/srv/madmail")
                && err.contains("--user-only"),
            "{err}"
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

        // An operator-chosen link to the state directory is resolved, not refused;
        // links inside it are still skipped.
        fs::write(root.join("sub/after-link"), "x").unwrap();
        let linked_root = outside.path().join("state-link");
        symlink(root, &linked_root).unwrap();
        chown_tree(&linked_root, uid, gid).unwrap();
        assert_eq!(owner(&root.join("sub/after-link")), (uid, gid));
        assert_eq!(owner(&target_file), before);
        assert_eq!(owner(&target_dir.join("inner")), before);
    }

    /// A real directory and its canonical path (macOS tempdirs live behind /var).
    fn real_tempdir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(dir.path()).unwrap();
        (dir, path)
    }

    #[test]
    fn tls_path_through_a_planted_link_in_the_state_dir_is_not_chowned() {
        let (_state, state) = real_tempdir();
        let (_outside, outside) = real_tempdir();
        fs::write(outside.join("fullchain.pem"), "x").unwrap();
        fs::write(outside.join("privkey.pem"), "x").unwrap();
        // The service account links an intermediate component out of its tree.
        symlink(&outside, state.join("tls")).unwrap();
        let cert = state.join("tls/fullchain.pem");
        let key = state.join("tls/privkey.pem");

        let tree = ServiceTree::new(&state);
        assert!(tree.contains(&cert) && tree.contains(&key));
        assert!(tree.contains(&state.join("tls")));
        let (uid, gid) = (fs::metadata(&outside).unwrap().uid(), 0);
        // Chowning would fail for a non-root caller (gid 0) if it were attempted.
        let skipped = chown_tls_files(&tree, &cert, &key, uid, gid).unwrap();
        assert_eq!(skipped, vec![cert, key]);

        // Paths outside the tree are not in it, even with the same file names.
        assert!(!tree.contains(&outside.join("fullchain.pem")));
    }

    #[test]
    fn canonical_tls_path_under_a_symlinked_state_root_is_in_the_tree() {
        let (_data, data) = real_tempdir();
        let (_links, links) = real_tempdir();
        // The operator's --state-dir is a link to a data volume.
        let state = links.join("state");
        symlink(&data, &state).unwrap();
        fs::create_dir(data.join("certs")).unwrap();
        fs::write(data.join("certs/fullchain.pem"), "x").unwrap();

        let tree = ServiceTree::new(&state);
        // Written in canonical form, so lexically outside the configured path.
        assert!(tree.contains(&data.join("certs/fullchain.pem")));
        // A file that does not exist yet is judged by its resolved parent.
        assert!(tree.contains(&data.join("certs/privkey.pem")));
        // A path elsewhere that resolves into the tree through a link.
        symlink(data.join("certs"), links.join("etc-certs")).unwrap();
        assert!(tree.contains(&links.join("etc-certs/fullchain.pem")));
        // The service account replaces the file with a link out of its tree: the
        // whole path now resolves elsewhere, but its parent is still in the tree.
        let (_outside, outside) = real_tempdir();
        fs::write(outside.join("shadow"), "x").unwrap();
        fs::remove_file(data.join("certs/fullchain.pem")).unwrap();
        symlink(outside.join("shadow"), data.join("certs/fullchain.pem")).unwrap();
        assert!(tree.contains(&links.join("etc-certs/fullchain.pem")));
        assert!(!tree.contains(&links.join("other.pem")));
        // A file-level operator link to a file in the tree, which the service
        // account then replaced with a link out of it: resolving the whole path
        // ends outside, but the first hop lands in the tree.
        symlink(data.join("certs/fullchain.pem"), links.join("op-cert.pem")).unwrap();
        assert_eq!(
            fs::canonicalize(links.join("op-cert.pem")).unwrap(),
            outside.join("shadow")
        );
        assert!(tree.contains(&links.join("op-cert.pem")));
        // The same through a relative link and a configured (not resolved) path.
        symlink("state/certs/fullchain.pem", links.join("op-rel.pem")).unwrap();
        assert!(tree.contains(&links.join("op-rel.pem")));
        // An operator link to a file outside the tree stays outside.
        symlink(outside.join("shadow"), links.join("op-out.pem")).unwrap();
        assert!(!tree.contains(&links.join("op-out.pem")));
        // A link loop ends after the hop limit instead of recursing forever.
        symlink(links.join("loop-b"), links.join("loop-a")).unwrap();
        symlink(links.join("loop-a"), links.join("loop-b")).unwrap();
        assert!(!tree.contains(&links.join("loop-a/x.pem")));
        // A missing state directory has no resolved form; only lexical checks apply.
        let missing = ServiceTree::new(Path::new("/nonexistent-madmail-state"));
        assert!(missing.contains(Path::new("/nonexistent-madmail-state/certs/a.pem")));
        assert!(!missing.contains(&data.join("certs/fullchain.pem")));
    }

    #[test]
    fn previous_home_that_resolves_to_the_state_dir_is_not_a_different_home() {
        let (_data, data) = real_tempdir();
        let (_links, links) = real_tempdir();
        let state = links.join("state");
        symlink(&data, &state).unwrap();

        assert!(same_dir(&state, &data) && same_dir(&data, &state));
        assert!(same_dir(&state, &state));
        assert!(!same_dir(&state, &links));
        assert!(same_dir(Path::new("/missing/a"), Path::new("/missing/a")));
        assert!(!same_dir(Path::new("/missing/a"), Path::new("/missing/b")));

        // passwd holds the canonical home while --state-dir is the link: the full
        // install neither refuses nor repoints the account.
        assert_eq!(home_for_comparison(&links, &state), links.as_path());
        let previous = home_for_comparison(&data, &state);
        let args = usermod_args(previous, &state, true, true, false, "madmail", "madmail");
        assert_eq!(
            args.unwrap(),
            ["-g", "madmail", "-s", "/sbin/nologin", "madmail"]
        );
    }
}
