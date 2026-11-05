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

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use chatmail_types::{ChatmailError, Result};

use super::chown_walk;
use super::config::InstallConfig;
use crate::ctl::host_paths;
use crate::ctl::secure_fs::{self, file_id, FileId, Resolved, Resolver, Trust, Want};

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
        ensure_state_parent(cfg)?;
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

/// Create the parent directories of `--state-dir` (as `mkdir -p` would) before
/// `useradd` / `usermod` set or move the home by path. They are created with
/// `mkdirat` in directories that passed the trust check (see [`Resolver`]);
/// every component, the parent itself included, must not be controlled by the
/// existing service account, so nothing it planted is followed and the account
/// is not changed when a component is refused. Without an account yet, only
/// world-writable directories without the sticky bit are refused.
fn ensure_state_parent(cfg: &InstallConfig) -> Result<()> {
    let Some(parent) = cfg.state_dir.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return Ok(());
    };
    let trust = match lookup_uid_gid(&cfg.maddy_user) {
        Ok((uid, gid)) => {
            Trust::service(uid, gid, None).with_groups(secure_fs::account_groups(&cfg.maddy_user))
        }
        // Uid and gid 0 are never counted as the service account.
        Err(_) => Trust::service(0, 0, None),
    };
    let resolver = Resolver {
        trust: &trust,
        state_root: None,
        check_final: true,
    };
    match resolver
        .ensure_dir(parent)
        .map_err(|e| io_error("mkdir", parent, e))?
    {
        Resolved::Found(_) => Ok(()),
        Resolved::Controlled { component, reason } => Err(ChatmailError::config(format!(
            "refusing to use state directory {}: {} is {reason}, so the service account \
             could redirect the account's home. Use a state directory whose parent \
             directories are owned by root",
            cfg.state_dir.display(),
            component.display()
        ))),
        Resolved::InTree(_) | Resolved::Missing { .. } => Err(io_error(
            "mkdir",
            parent,
            io::Error::from(io::ErrorKind::NotFound),
        )),
    }
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

/// Root's handle on a system install that has a service account: the service
/// identity, the trust rule built from it and the open state directory.
///
/// Every file root creates or changes for the install goes through it:
/// - paths are resolved with [`Resolver`] (no link is followed unless every
///   component on the way is trusted, see [`secure_fs`]);
/// - a path that reaches the state directory is handled below the opened state
///   root with `O_NOFOLLOW` at every component, and files there are written by
///   create-new-then-rename, so links and hard links the service planted are
///   replaced, never written through;
/// - owners and modes are changed with `fchown` / `fchmod` on a descriptor
///   for the object that was checked;
/// - a file written inside the state tree is given its owner on the new
///   file's descriptor before it is renamed into place: the service account
///   when install manages ownership, so the state walk only has to change
///   files that already belong to the service (see [`chown_walk::decide`]).
pub struct SecureInstall {
    pub uid: u32,
    pub gid: u32,
    /// The state directory's owner before this run when it is neither root nor
    /// `uid` (the previous service uid after the account was recreated), as
    /// the state walk uses it: left out when that uid now belongs to another
    /// account, whose files are then not handed to the service. The trust rule
    /// counts the previous owner as untrusted in both cases.
    walk_old_uid: Option<u32>,
    /// The service account's groups (primary and supplementary).
    groups: Vec<u32>,
    trust: Trust,
    state_root: OwnedFd,
    state_id: FileId,
    /// `--state-dir` as given (normalized), to tell whether a config path was
    /// chosen inside the state directory.
    state_path: std::path::PathBuf,
    primary: String,
    /// Whether install changes owners and modes (not with `--skip-user`).
    manage_ownership: bool,
    /// Account and group names, for the commands printed to the operator.
    user: String,
    group: String,
}

impl SecureInstall {
    /// `None` for local installs, and for `--skip-user` when the service
    /// account does not exist (there is then no account to guard against). With
    /// `--skip-user` and an existing account every write still goes through
    /// this handle; only the account and ownership steps are skipped. `create`
    /// makes a missing state directory (and its parents) in trusted
    /// directories.
    pub fn open(cfg: &InstallConfig, create: bool) -> Result<Option<Self>> {
        if !cfg.system_install {
            return Ok(None);
        }
        let (uid, gid) = match lookup_uid_gid(&cfg.maddy_user) {
            Ok(ids) => ids,
            Err(_) if cfg.skip_user => {
                if let Ok(m) = fs::metadata(&cfg.state_dir) {
                    use std::os::unix::fs::MetadataExt;
                    if m.uid() != 0 {
                        return Err(ChatmailError::config(format!(
                            "--skip-user: service account {} does not exist, but state \
                             directory {} exists and belongs to uid {}, not root. Install \
                             would write into a directory another account controls; run \
                             install without --skip-user, or make the directory root-owned",
                            cfg.maddy_user,
                            cfg.state_dir.display(),
                            m.uid()
                        )));
                    }
                }
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        let groups = secure_fs::account_groups(&cfg.maddy_user);
        let first = Trust::service(uid, gid, None).with_groups(groups.iter().copied());
        let (_, st) = open_state_root(&cfg.state_dir, &first, create)?;
        let old_uid = (st.st_uid != 0 && st.st_uid != uid).then_some(st.st_uid);
        let walk_old_uid = old_uid.filter(|&old| match secure_fs::account_name(old) {
            Some(name) if name != cfg.maddy_user => {
                println!(
                    "   ! {} belonged to uid {old}, which is now account {name}: files and \
                     directories of that uid in it are not given to {}",
                    cfg.state_dir.display(),
                    cfg.maddy_user
                );
                false
            }
            _ => true,
        });
        // Check the way to the state directory again with the old uid counted.
        let trust = Trust::service(uid, gid, old_uid).with_groups(groups.iter().copied());
        let (state_root, st) = open_state_root(&cfg.state_dir, &trust, false)?;
        Ok(Some(Self {
            uid,
            gid,
            walk_old_uid,
            groups,
            trust,
            state_root,
            state_id: file_id(&st),
            state_path: host_paths::normalize(&cfg.state_dir),
            primary: cfg.binary_name.clone(),
            manage_ownership: !cfg.skip_user,
            user: cfg.maddy_user.clone(),
            group: cfg.maddy_group.clone(),
        }))
    }

    fn resolver(&self, check_final: bool) -> Resolver<'_> {
        Resolver {
            trust: &self.trust,
            state_root: Some(self.state_id),
            check_final,
        }
    }

    /// The owner a file written inside the state tree gets on its new
    /// descriptor: the service account when install manages ownership. With
    /// `--skip-user` the service account only when the regular file being
    /// replaced (looked at through the directory descriptor `dir`, without
    /// following a link) belongs to it, as an in-place rewrite would keep it;
    /// root otherwise.
    fn in_tree_owner(&self, dir: RawFd, name: &OsStr) -> Option<(u32, u32)> {
        if self.manage_ownership {
            return Some((self.uid, self.gid));
        }
        let c_name = secure_fs::cstring(name).ok()?;
        let st = secure_fs::fstatat_nofollow(dir, &c_name).ok()?;
        (secure_fs::kind(&st) == libc::S_IFREG && st.st_uid == self.uid)
            .then_some((self.uid, self.gid))
    }

    /// Whether `uid` is the service account's (current or previous uid).
    fn is_service_uid(&self, uid: u32) -> bool {
        uid == self.uid || Some(uid) == self.walk_old_uid
    }

    /// Whether the state directory itself is controlled by the service account
    /// (owned by it, or writable by one of its groups).
    pub fn state_dir_is_service_controlled(&self) -> bool {
        secure_fs::fstat(self.state_root.as_raw_fd())
            .map(|st| self.trust.verdict(&st).is_some())
            .unwrap_or(true)
    }

    /// Whether `path` was given inside `--state-dir` (lexically).
    fn chosen_in_state_dir(&self, path: &Path) -> bool {
        host_paths::normalize(path).starts_with(&self.state_path)
    }

    /// Create the directory `path` (and missing parents) unless it exists.
    pub fn ensure_dir(&self, path: &Path) -> Result<()> {
        match self
            .resolver(false)
            .ensure_dir(path)
            .map_err(|e| io_error("mkdir", path, e))?
        {
            Resolved::Found(_) => Ok(()),
            Resolved::InTree(rest) => secure_fs::tree_open_dir(&self.state_root, &rest, true)
                .map(drop)
                .map_err(|e| in_tree_error("mkdir", path, &rest, e)),
            Resolved::Controlled { component, reason } => {
                Err(controlled_error("create", path, &component, reason))
            }
            Resolved::Missing { .. } => Err(io_error(
                "mkdir",
                path,
                io::Error::from(io::ErrorKind::NotFound),
            )),
        }
    }

    /// Write `data` to `path` with `mode`, replacing what is there (see
    /// [`secure_fs::replace_file`]). Operator links on the way are followed
    /// through the checked resolver. `owner` is applied outside the state tree;
    /// inside it the owner is [`Self::in_tree_owner`].
    pub fn write_file(
        &self,
        path: &Path,
        data: &[u8],
        mode: u32,
        owner: Option<(u32, u32)>,
    ) -> Result<()> {
        self.place_file(path, data, mode, owner, false, false)
            .map(drop)
    }

    /// Write the config file (mode 0640). A config path given outside
    /// `--state-dir` that resolves into the state directory through a link is
    /// refused: the service account would control the file root's commands
    /// read by default.
    pub fn write_config_file(
        &self,
        path: &Path,
        data: &[u8],
        owner: Option<(u32, u32)>,
    ) -> Result<()> {
        self.place_file(path, data, 0o640, owner, false, true)
            .map(drop)
    }

    /// Like [`Self::write_file`], but only when nothing exists at the final
    /// location; returns false when something does.
    pub fn create_new_file(&self, path: &Path, data: &[u8], mode: u32) -> Result<bool> {
        self.place_file(path, data, mode, None, true, false)
    }

    fn place_file(
        &self,
        path: &Path,
        data: &[u8],
        mode: u32,
        owner: Option<(u32, u32)>,
        create_new: bool,
        config: bool,
    ) -> Result<bool> {
        let write = |dir: RawFd, name: &OsStr, owner| {
            Ok::<bool, io::Error>(if create_new {
                secure_fs::create_new_file(dir, name, data, mode, owner)?.is_some()
            } else {
                secure_fs::replace_file(dir, name, data, mode, owner)?;
                true
            })
        };
        let in_tree_allowed = || {
            if config && !self.chosen_in_state_dir(path) {
                Err(config_into_tree_error(path))
            } else {
                Ok(())
            }
        };
        for _ in 0..2 {
            match self
                .resolver(false)
                .resolve(path, Want::File)
                .map_err(|e| io_error("resolve", path, e))?
            {
                Resolved::InTree(rest) => {
                    in_tree_allowed()?;
                    let (dir, name) = secure_fs::split_file(&rest)
                        .map_err(|e| in_tree_error("write", path, &rest, e))?;
                    let fd = secure_fs::tree_open_dir(&self.state_root, &dir, true)
                        .map_err(|e| in_tree_error("write", path, &rest, e))?;
                    let owner = self.in_tree_owner(fd.as_raw_fd(), &name);
                    return write(fd.as_raw_fd(), &name, owner)
                        .map_err(|e| in_tree_error("write", path, &rest, e));
                }
                Resolved::Found(file) => {
                    if create_new {
                        return Ok(false);
                    }
                    // Replace the file where the operator's links lead; its
                    // directory must itself be trusted.
                    let parent = file.path.parent().unwrap_or(Path::new("/"));
                    let name = file.path.file_name().unwrap_or_default().to_os_string();
                    return match self
                        .resolver(true)
                        .resolve(parent, Want::Dir)
                        .map_err(|e| io_error("resolve", parent, e))?
                    {
                        Resolved::Found(dir) => write(dir.fd.as_raw_fd(), &name, owner)
                            .map_err(|e| io_error("write", &file.path, e)),
                        Resolved::InTree(rest) => {
                            in_tree_allowed()?;
                            let fd = secure_fs::tree_open_dir(&self.state_root, &rest, true)
                                .map_err(|e| in_tree_error("write", path, &rest, e))?;
                            let owner = self.in_tree_owner(fd.as_raw_fd(), &name);
                            write(fd.as_raw_fd(), &name, owner)
                                .map_err(|e| in_tree_error("write", path, &rest, e))
                        }
                        Resolved::Controlled { component, reason } => {
                            Err(controlled_error("write", path, &component, reason))
                        }
                        Resolved::Missing { .. } => Err(io_error(
                            "write",
                            path,
                            io::Error::from(io::ErrorKind::NotFound),
                        )),
                    };
                }
                Resolved::Missing { parent, name, rest } if rest.as_os_str().is_empty() => {
                    // `parent` passed the trust check as an intermediate directory.
                    return write(parent.fd.as_raw_fd(), &name, owner)
                        .map_err(|e| io_error("write", path, e));
                }
                Resolved::Missing { .. } => {
                    if let Some(parent) = path.parent() {
                        self.ensure_dir(parent)?;
                    }
                }
                Resolved::Controlled { component, reason } => {
                    return Err(controlled_error("write", path, &component, reason));
                }
            }
        }
        Err(io_error(
            "write",
            path,
            io::Error::from(io::ErrorKind::NotFound),
        ))
    }

    /// The owner of the regular file at `path` when it is reached through
    /// trusted components outside the state tree; `None` otherwise.
    pub fn existing_owner(&self, path: &Path) -> Option<(u32, u32)> {
        match self.resolver(false).resolve(path, Want::File) {
            Ok(Resolved::Found(file)) => Some((file.st.st_uid, file.st.st_gid)),
            _ => None,
        }
    }

    /// Read a regular file with one link (at most `max` bytes); `None` when it
    /// does not exist. Inside the state tree the file must also belong to the
    /// service account (or, with `--skip-user`, to root), so root never copies
    /// out the contents of another account's file that a hard link put there.
    pub fn read_file(&self, path: &Path, max: usize) -> Result<Option<Vec<u8>>> {
        let opened = match self
            .resolver(false)
            .resolve(path, Want::File)
            .map_err(|e| io_error("resolve", path, e))?
        {
            Resolved::Missing { .. } => return Ok(None),
            Resolved::Controlled { component, reason } => {
                return Err(controlled_error("read", path, &component, reason))
            }
            Resolved::Found(file) => {
                if file.st.st_nlink > 1 {
                    return Err(io_error(
                        "read",
                        path,
                        io::Error::new(io::ErrorKind::InvalidInput, "has several hard links"),
                    ));
                }
                Some(file.fd)
            }
            Resolved::InTree(rest) => {
                let (dir, name) = secure_fs::split_file(&rest)
                    .map_err(|e| in_tree_error("read", path, &rest, e))?;
                let fd = match secure_fs::tree_open_dir(&self.state_root, &dir, false) {
                    Ok(fd) => fd,
                    Err(e) if e.raw_os_error() == Some(libc::ENOENT) => return Ok(None),
                    Err(e) => return Err(in_tree_error("read", path, &rest, e)),
                };
                match secure_fs::open_single_link_file(fd.as_raw_fd(), &name)
                    .map_err(|e| in_tree_error("read", path, &rest, e))?
                {
                    None => None,
                    Some((_, st))
                        if !self.is_service_uid(st.st_uid)
                            && !(st.st_uid == 0 && !self.manage_ownership) =>
                    {
                        return Err(ChatmailError::config(format!(
                            "refusing to read {} as root: it belongs to uid {}, not to the \
                             service account, so it may be another account's file linked \
                             there. Check it; if it is madmail's own file, run: chown {}:{} {}",
                            path.display(),
                            st.st_uid,
                            self.user,
                            self.group,
                            shell_quote(path)
                        )));
                    }
                    Some((fd, _)) => Some(fd),
                }
            }
        };
        match opened {
            None => Ok(None),
            Some(fd) => secure_fs::read_fd(&fd, max)
                .map(Some)
                .map_err(|e| io_error("read", path, e)),
        }
    }

    /// Give the config file (which holds `ss_password` and `turn_secret`) mode
    /// 0640: owned by `root:gid` outside the state tree, by `uid:gid` inside it
    /// (install writes it with that owner). A config file reached through a
    /// component the service account controls, outside the state tree, is an
    /// error, and so is one given outside `--state-dir` that resolves into it.
    /// Install runs this after the state walk, so this is the config file's
    /// final owner and mode.
    pub fn fix_config_file(&self, path: &Path) -> Result<()> {
        match self
            .resolver(true)
            .resolve(path, Want::File)
            .map_err(|e| io_error("resolve", path, e))?
        {
            Resolved::Missing { .. } => Ok(()),
            Resolved::Controlled { component, reason } => Err(ChatmailError::config(format!(
                "refusing to change config file {}: {} is {reason}, so the service account \
                 could redirect a change made by path. Keep the config file in a root-owned \
                 directory (or under --state-dir) and re-run",
                path.display(),
                component.display()
            ))),
            Resolved::Found(file) => {
                if file.st.st_nlink > 1 {
                    return Err(ChatmailError::config(format!(
                        "refusing to change config file {}: it has {} hard links",
                        path.display(),
                        file.st.st_nlink
                    )));
                }
                let fd = file.fd.as_raw_fd();
                secure_fs::fchown(fd, 0, self.gid).map_err(|e| io_error("chown", path, e))?;
                secure_fs::fchmod(fd, 0o640).map_err(|e| io_error("chmod", path, e))
            }
            Resolved::InTree(rest) => {
                if !self.chosen_in_state_dir(path) {
                    return Err(config_into_tree_error(path));
                }
                let (dir, name) = secure_fs::split_file(&rest)
                    .map_err(|e| in_tree_error("chmod", path, &rest, e))?;
                let dir = secure_fs::tree_open_dir(&self.state_root, &dir, false)
                    .map_err(|e| in_tree_error("chmod", path, &rest, e))?;
                let (fd, st) = secure_fs::open_regular_file(dir.as_raw_fd(), &name)
                    .and_then(|f| f.ok_or_else(|| io::Error::from_raw_os_error(libc::ENOENT)))
                    .map_err(|e| in_tree_error("chmod", path, &rest, e))?;
                if !self.is_service_uid(st.st_uid) {
                    return Err(ChatmailError::config(format!(
                        "refusing to change config file {}: it belongs to uid {}, not to the \
                         service account",
                        path.display(),
                        st.st_uid
                    )));
                }
                secure_fs::fchmod(fd.as_raw_fd(), 0o640).map_err(|e| io_error("chmod", path, e))
            }
        }
    }

    /// Give a TLS file to `uid:gid` through a descriptor, or say why not. A
    /// file outside the state tree reached through trusted directories only
    /// was placed by root or the operator; it is changed unless it has more
    /// than one hard link and belongs to another account.
    pub fn fix_tls_file(&self, path: &Path) -> Result<()> {
        match self
            .resolver(false)
            .resolve(path, Want::File)
            .map_err(|e| io_error("resolve", path, e))?
        {
            Resolved::Missing { .. } => {}
            Resolved::InTree(rest) => self.note_in_tree_tls_file(path, &rest),
            Resolved::Controlled { component, reason } => skip_note(
                path,
                &format!(
                    "{} is {reason}; the service account could redirect a change made by path",
                    component.display()
                ),
            ),
            Resolved::Found(file) => {
                let owner = file.st.st_uid;
                let service_owned = owner == self.uid || Some(owner) == self.walk_old_uid;
                if file.st.st_nlink > 1 && !service_owned {
                    skip_note(
                        path,
                        &format!(
                            "{} has {} hard links and belongs to uid {owner}; it may be a \
                             link to another account's file",
                            file.path.display(),
                            file.st.st_nlink
                        ),
                    );
                } else {
                    secure_fs::fchown(file.fd.as_raw_fd(), self.uid, self.gid)
                        .map_err(|e| io_error("chown", path, e))?;
                }
            }
        }
        Ok(())
    }

    /// A TLS file inside the state tree is not changed by path. It is looked
    /// at through the tree descriptors without following a link: one that
    /// belongs to the service account was handled by the state walk (install
    /// writes its own TLS files there with that owner); for anything else the
    /// operator is told what to check and change.
    fn note_in_tree_tls_file(&self, path: &Path, rest: &Path) {
        let st = secure_fs::split_file(rest).and_then(|(dir, name)| {
            let dir = secure_fs::tree_open_dir(&self.state_root, &dir, false)?;
            secure_fs::fstatat_nofollow(dir.as_raw_fd(), &secure_fs::cstring(&name)?)
        });
        let why = match st {
            Err(e) if e.raw_os_error() == Some(libc::ENOENT) => return,
            Err(e) => format!(
                "inside the state directory, and it cannot be reached there without following \
                 a link ({e})"
            ),
            Ok(st) if secure_fs::kind(&st) == libc::S_IFLNK => {
                "a link inside the state directory; links there are not followed".to_string()
            }
            Ok(st) if self.is_service_uid(st.st_uid) => return,
            Ok(st) => format!(
                "inside the state directory and belongs to uid {}, not to the service account, \
                 so the state walk leaves it unchanged. If it is your certificate or key, check \
                 that it is not a link to another file, then run: chown {}:{} {}",
                st.st_uid,
                self.user,
                self.group,
                shell_quote(path)
            ),
        };
        skip_note(path, &why);
    }

    /// Re-own a config or certs directory as `root:gid` and clear its group
    /// and other write bits, through its descriptor, when it is dedicated to
    /// madmail; otherwise leave it alone. Both the path as given and the
    /// directory actually opened (after the operator's links) must pass
    /// [`host_paths::is_dedicated_dir`], and the opened directory must not be
    /// one of [`host_paths::DENIED_DIRS`] by device and inode.
    pub fn fix_dir(&self, dir: &Path) -> Result<()> {
        match self
            .resolver(true)
            .resolve(dir, Want::Dir)
            .map_err(|e| io_error("resolve", dir, e))?
        {
            Resolved::Missing { .. } | Resolved::InTree(_) => {}
            Resolved::Controlled { component, reason } => skip_note(
                dir,
                &format!(
                    "{} is {reason}; the service account could redirect a change made by path",
                    component.display()
                ),
            ),
            Resolved::Found(opened)
                if host_paths::is_dedicated_dir(dir, &self.primary)
                    && host_paths::is_dedicated_dir(&opened.path, &self.primary)
                    && !host_paths::is_denied_dir_id(file_id(&opened.st)) =>
            {
                let fd = opened.fd.as_raw_fd();
                secure_fs::fchown(fd, 0, self.gid).map_err(|e| io_error("chown", dir, e))?;
                secure_fs::fchmod(fd, secure_fs::perm_bits(&opened.st) & !0o022)
                    .map_err(|e| io_error("chmod", dir, e))?;
            }
            Resolved::Found(opened) => {
                let resolved = if opened.path != host_paths::normalize(dir) {
                    format!(" (resolves to {})", opened.path.display())
                } else {
                    String::new()
                };
                println!(
                    "   Leaving owner and mode of {}{resolved} unchanged: not a directory \
                     dedicated to madmail",
                    dir.display()
                )
            }
        }
        Ok(())
    }

    /// A new descriptor for the state directory, for the ownership walk.
    fn state_root_fd(&self) -> io::Result<OwnedFd> {
        self.state_root.try_clone()
    }
}

fn config_into_tree_error(path: &Path) -> ChatmailError {
    ChatmailError::config(format!(
        "refusing to use config file {}: it is outside --state-dir but resolves into the \
         state directory through a link, so the service account would control it. Point \
         the link elsewhere, or give a --config-dir under --state-dir",
        path.display()
    ))
}

/// A private directory (mode 0700, owned by root) in the system temporary
/// directory, removed on drop. The temporary directory itself must pass the
/// root-only trust check (root-owned; `/tmp` with its sticky bit passes), so
/// no other account can rename or replace the staging directory.
pub struct StagingDir {
    path: std::path::PathBuf,
}

impl StagingDir {
    pub fn create(sec: &SecureInstall) -> Result<Self> {
        use std::os::unix::fs::DirBuilderExt;
        let base = std::env::temp_dir();
        let trust = Trust::root_only(sec.groups.iter().copied());
        let resolver = Resolver {
            trust: &trust,
            state_root: None,
            check_final: true,
        };
        match resolver
            .resolve(&base, Want::Dir)
            .map_err(|e| io_error("resolve", &base, e))?
        {
            Resolved::Found(_) => {}
            Resolved::Controlled { component, reason } => {
                return Err(controlled_error(
                    "stage files in",
                    &base,
                    &component,
                    reason,
                ))
            }
            _ => {
                return Err(io_error(
                    "resolve",
                    &base,
                    io::Error::from(io::ErrorKind::NotFound),
                ))
            }
        }
        for _ in 0..8 {
            let mut bytes = [0u8; 8];
            getrandom::fill(&mut bytes)
                .map_err(|e| ChatmailError::config(format!("random name: {e}")))?;
            let suffix: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            let path = base.join(format!(".madmail-install-{suffix}"));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io_error("mkdir", &path, e)),
            }
        }
        Err(ChatmailError::config(
            "could not create a staging directory for the certificate",
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Resolve and open the state directory. Its own owner is not checked (it is
/// the service account's), but every component on the way to it is.
fn open_state_root(path: &Path, trust: &Trust, create: bool) -> Result<(OwnedFd, libc::stat)> {
    let resolver = Resolver {
        trust,
        state_root: None,
        check_final: false,
    };
    let resolved = if create {
        resolver.ensure_dir(path)
    } else {
        resolver.resolve(path, Want::Dir)
    }
    .map_err(|e| io_error("open state directory", path, e))?;
    match resolved {
        Resolved::Found(dir) => Ok((dir.fd, dir.st)),
        Resolved::Controlled { component, reason } => Err(ChatmailError::config(format!(
            "refusing to use state directory {}: {} is {reason}, so the service account \
             could swap the directory root works in. Use a state directory whose parent \
             directories are owned by root",
            path.display(),
            component.display()
        ))),
        Resolved::Missing { .. } | Resolved::InTree(_) => Err(io_error(
            "open state directory",
            path,
            io::Error::from(io::ErrorKind::NotFound),
        )),
    }
}

fn controlled_error(op: &str, path: &Path, component: &Path, reason: &str) -> ChatmailError {
    ChatmailError::config(format!(
        "refusing to {op} {} as root: {} is {reason}",
        path.display(),
        component.display()
    ))
}

fn in_tree_error(op: &str, path: &Path, rest: &Path, e: io::Error) -> ChatmailError {
    ChatmailError::config(format!(
        "{op} {} (state directory entry {}): {e}; a link or non-directory inside the \
         state directory is not followed",
        path.display(),
        rest.display()
    ))
}

/// TLS material and config must be readable by the service user. See
/// [`SecureInstall`] for how each path is checked and changed. Nothing is
/// changed with `--skip-user`.
pub fn setup_config_permissions(cfg: &InstallConfig, sec: Option<&SecureInstall>) -> Result<()> {
    let Some(sec) = sec.filter(|s| s.manage_ownership) else {
        return Ok(());
    };
    let conf_dir = cfg.config_path.parent();
    if let Some(dir) = conf_dir {
        sec.fix_dir(dir)?;
    }
    sec.fix_config_file(&cfg.config_path)?;
    for path in [&cfg.cert_path, &cfg.key_path] {
        sec.fix_tls_file(path)?;
    }
    if let Some(certs) = cfg.cert_path.parent().filter(|c| Some(*c) != conf_dir) {
        sec.fix_dir(certs)?;
    }
    Ok(())
}

/// `path` quoted for a POSIX shell command shown to the operator.
fn shell_quote(path: &Path) -> String {
    let text = path.to_string_lossy();
    if !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-+:@".contains(&b))
    {
        return text.into_owned();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn skip_note(path: &Path, why: &str) {
    println!("   ! Skipping ownership of {}: {why}", path.display());
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

/// The directory for the binary must be trusted and outside the state
/// directory: root later runs the binary, and the systemd unit starts it. With
/// a service account the check uses its trust rule; without one (local mode is
/// not handled here) the root-only rule.
fn binary_dir_resolver<'a>(sec: Option<&'a SecureInstall>, fallback: &'a Trust) -> Resolver<'a> {
    match sec {
        Some(sec) => sec.resolver(true),
        None => Resolver {
            trust: fallback,
            state_root: None,
            check_final: true,
        },
    }
}

fn binary_dir_refusal(cfg: &InstallConfig, why: &str) -> ChatmailError {
    ChatmailError::config(format!(
        "refusing to install the binary at {}: {why}. Choose a --binary-path in a \
         root-owned directory outside --state-dir",
        cfg.binary_path.display()
    ))
}

/// Refuse a `--binary-path` inside `--state-dir` (as written) and, with a
/// service account, one whose directory resolves into the state directory or
/// is reached through a component the service account controls. Install runs
/// it before it changes anything; [`install_binary`] checks again.
pub fn check_binary_path(cfg: &InstallConfig, sec: Option<&SecureInstall>) -> Result<()> {
    if !cfg.system_install {
        return Ok(());
    }
    if host_paths::normalize(&cfg.binary_path).starts_with(host_paths::normalize(&cfg.state_dir)) {
        return Err(binary_dir_refusal(
            cfg,
            "it is inside the state directory, which the service account can change",
        ));
    }
    let (Some(sec), Some(parent)) = (sec, cfg.binary_path.parent()) else {
        return Ok(());
    };
    match sec
        .resolver(true)
        .resolve(parent, Want::Dir)
        .map_err(|e| io_error("resolve", parent, e))?
    {
        Resolved::Found(_) | Resolved::Missing { .. } => Ok(()),
        Resolved::InTree(_) => Err(binary_dir_refusal(
            cfg,
            "its directory resolves into the state directory, which the service account can change",
        )),
        Resolved::Controlled { component, reason } => Err(binary_dir_refusal(
            cfg,
            &format!("{} is {reason}", component.display()),
        )),
    }
}

/// Copy the running binary to `--binary-path` (mode 0755, `root:root`): the
/// directory is resolved and created with the checked resolver, and the file
/// is written by create-new-then-rename through that directory's descriptor
/// ([`secure_fs::replace_file_with`]), so a link at the destination is
/// replaced, never written through.
pub fn install_binary(
    cfg: &InstallConfig,
    sec: Option<&SecureInstall>,
    dry_run: bool,
) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
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

    check_binary_path(cfg, sec)?;
    let (Some(parent), Some(name)) = (cfg.binary_path.parent(), cfg.binary_path.file_name()) else {
        return Err(binary_dir_refusal(cfg, "it does not name a file"));
    };
    let fallback = Trust::root_only(Vec::new());
    let dir = match binary_dir_resolver(sec, &fallback)
        .ensure_dir(parent)
        .map_err(|e| io_error("mkdir", parent, e))?
    {
        Resolved::Found(dir) => dir,
        Resolved::InTree(_) => return Err(binary_dir_refusal(
            cfg,
            "its directory resolves into the state directory, which the service account can change",
        )),
        Resolved::Controlled { component, reason } => {
            return Err(binary_dir_refusal(
                cfg,
                &format!("{} is {reason}", component.display()),
            ))
        }
        Resolved::Missing { .. } => {
            return Err(io_error(
                "mkdir",
                parent,
                io::Error::from(io::ErrorKind::NotFound),
            ))
        }
    };
    let running = fs::File::open(&current).map_err(|e| io_error("open", &current, e))?;
    let running_meta = running
        .metadata()
        .map_err(|e| io_error("stat", &current, e))?;
    let c_name = secure_fs::cstring(name).map_err(|e| io_error("install", &cfg.binary_path, e))?;
    let existing = secure_fs::fstatat_nofollow(dir.fd.as_raw_fd(), &c_name).ok();
    let same = existing.is_some_and(|st| {
        secure_fs::kind(&st) == libc::S_IFREG
            && file_id(&st) == (running_meta.dev(), running_meta.ino())
    });
    if same {
        println!(
            "   Skipping binary install (already running from {})",
            cfg.binary_path.display()
        );
        let fd = secure_fs::openat(dir.fd.as_raw_fd(), &c_name, secure_fs::FILE_FLAGS, 0)
            .map_err(|e| io_error("open", &cfg.binary_path, e))?;
        secure_fs::fchmod(fd.as_raw_fd(), 0o755)
            .map_err(|e| io_error("chmod", &cfg.binary_path, e))?;
    } else {
        println!(
            "   Installing {} → {}",
            current.display(),
            cfg.binary_path.display()
        );
        secure_fs::replace_file_with(dir.fd.as_raw_fd(), name, 0o755, Some((0, 0)), |fd| {
            // SAFETY: `fd` stays open for the closure; ManuallyDrop keeps the
            // File from closing it.
            let mut out = std::mem::ManuallyDrop::new(unsafe {
                <fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd)
            });
            let mut input = &running;
            io::copy(&mut input, &mut *out).map(drop)
        })
        .map_err(|e| io_error("install binary", &cfg.binary_path, e))?;
    }
    println!("   ✓ Binary installed");
    Ok(())
}

/// Give the state tree to the service account with the descriptor walk (see
/// [`chown_walk`]). The walk starts from the state directory opened by
/// [`SecureInstall::open`], so an operator's link to a data volume was followed
/// only through the checked resolver. Files and directories of other accounts
/// are listed and left alone, each file with the `chown` command the operator
/// can run after checking it. Nothing is changed with `--skip-user`.
pub fn setup_permissions(cfg: &InstallConfig, sec: Option<&SecureInstall>) -> Result<()> {
    let Some(sec) = sec.filter(|s| s.manage_ownership) else {
        return Ok(());
    };
    println!(
        "   Setting ownership {}:{} on {}",
        cfg.maddy_user,
        cfg.maddy_group,
        cfg.state_dir.display()
    );
    let fd = sec
        .state_root_fd()
        .map_err(|e| io_error("open", &cfg.state_dir, e))?;
    let skipped = chown_walk::chown_tree_fd(fd, &cfg.state_dir, sec.uid, sec.gid, sec.walk_old_uid)
        .map_err(|e| io_error(e.op, &e.path, e.source))?;
    const SHOWN: usize = 50;
    for entry in skipped.iter().take(SHOWN) {
        println!(
            "   ! Ownership of {} left unchanged: {}",
            entry.path.display(),
            entry.reason
        );
        if entry.foreign_owner {
            println!(
                "     If it is madmail's own (check it is not a hard link to another file), run: \
                 chown {}:{} {}",
                sec.user,
                sec.group,
                shell_quote(&entry.path)
            );
        }
    }
    if skipped.len() > SHOWN {
        println!(
            "   ! ... and {} more entries left unchanged",
            skipped.len() - SHOWN
        );
    }
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
    use std::path::PathBuf;

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

    /// No test file has this uid: a chown to it by a non-root caller fails
    /// with EPERM, so a test that passes shows no chown was attempted.
    const UNUSED_UID: u32 = 4_000_000_000;

    /// A directory under the root-owned, sticky `/tmp`, by its canonical path.
    fn base() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir_in("/tmp").unwrap();
        let path = fs::canonicalize(dir.path()).unwrap();
        (dir, path)
    }

    fn id(path: &Path) -> FileId {
        let m = fs::symlink_metadata(path).unwrap();
        (m.dev(), m.ino())
    }

    /// A handle as `SecureInstall::open` builds it, for a service account that
    /// does not exist; `marked` objects count as owned by the service.
    fn handle(state: &Path, old_uid: Option<u32>, marked: &[&Path]) -> SecureInstall {
        let marked: Vec<FileId> = marked.iter().map(|p| id(p)).collect();
        // The test's base directory is the caller's: open the state root
        // without counting the caller as the old uid, as for a root-owned path.
        let opening = Trust::service(UNUSED_UID, UNUSED_UID, None).with_marked(marked.clone());
        let (state_root, st) = open_state_root(state, &opening, false).unwrap();
        let trust = Trust::service(UNUSED_UID, UNUSED_UID, old_uid).with_marked(marked);
        SecureInstall {
            uid: UNUSED_UID,
            gid: UNUSED_UID,
            walk_old_uid: old_uid,
            groups: Vec::new(),
            trust,
            state_root,
            state_id: file_id(&st),
            state_path: host_paths::normalize(state),
            primary: "madmail".into(),
            manage_ownership: true,
            user: "madmail".into(),
            group: "madmail".into(),
        }
    }

    /// [`handle`] whose service account is the caller (the trust rule still
    /// uses `UNUSED_UID`): files written into the tree get the caller as
    /// owner, which a non-root caller may set.
    fn caller_handle(state: &Path, marked: &[&Path]) -> SecureInstall {
        let mut sec = handle(state, None, marked);
        // SAFETY: getuid/getgid have no preconditions.
        (sec.uid, sec.gid) = unsafe { (libc::getuid(), libc::getgid()) };
        sec
    }

    #[test]
    fn in_tree_files_get_the_service_owner_on_creation() {
        let (_b, b) = base();
        let state = b.join("state");
        fs::create_dir_all(state.join("dkim")).unwrap();
        let sec = caller_handle(&state, &[&state]);
        let dkim = secure_fs::tree_open_dir(&sec.state_root, Path::new("dkim"), false).unwrap();
        let name = OsStr::new("default.txt");
        // Install manages ownership: always the service account.
        assert_eq!(
            sec.in_tree_owner(dkim.as_raw_fd(), name),
            Some((sec.uid, sec.gid))
        );
        sec.write_file(&state.join("dkim/default.txt"), b"txt", 0o644, None)
            .unwrap();
        let m = fs::metadata(state.join("dkim/default.txt")).unwrap();
        assert_eq!((m.uid(), m.gid()), (sec.uid, sec.gid));
        // --skip-user: the service account only when the replaced regular file
        // is its own; root (no owner change) for a missing entry, a link or
        // another account's file.
        let mut skip = caller_handle(&state, &[&state]);
        skip.manage_ownership = false;
        assert_eq!(
            skip.in_tree_owner(dkim.as_raw_fd(), name),
            Some((skip.uid, skip.gid))
        );
        assert_eq!(
            skip.in_tree_owner(dkim.as_raw_fd(), OsStr::new("absent")),
            None
        );
        symlink(state.join("dkim/default.txt"), state.join("dkim/link")).unwrap();
        assert_eq!(
            skip.in_tree_owner(dkim.as_raw_fd(), OsStr::new("link")),
            None
        );
        skip.uid = UNUSED_UID;
        assert_eq!(skip.in_tree_owner(dkim.as_raw_fd(), name), None);
        // With the UNUSED_UID service account a write into the tree attempts
        // the chown on the new file, which fails for a non-root caller: the
        // owner is set before the file is renamed into place.
        // SAFETY: getuid has no preconditions.
        if unsafe { libc::getuid() } != 0 {
            let foreign = handle(&state, None, &[&state]);
            let err = foreign
                .write_file(&state.join("dkim/default.txt"), b"new", 0o644, None)
                .unwrap_err()
                .to_string();
            assert!(err.contains("Operation not permitted"), "{err}");
            assert_eq!(
                fs::read_to_string(state.join("dkim/default.txt")).unwrap(),
                "txt"
            );
        }
    }

    #[test]
    fn state_walk_from_the_resolved_root_never_follows_links() {
        let (_b, b) = base();
        let outside = b.join("outside");
        fs::create_dir_all(outside.join("dir")).unwrap();
        fs::write(outside.join("dir/inner"), "x").unwrap();
        fs::write(outside.join("file"), "x").unwrap();
        let state = b.join("data");
        fs::create_dir_all(state.join("sub")).unwrap();
        fs::write(state.join("sub/data"), "x").unwrap();
        symlink(outside.join("dir"), state.join("dir-link")).unwrap();
        symlink(outside.join("file"), state.join("file-link")).unwrap();
        symlink(state.join("missing"), state.join("dangling")).unwrap();
        // The operator's --state-dir is a link to the data directory.
        symlink(&state, b.join("state-link")).unwrap();
        let sec = handle(&b.join("state-link"), None, &[&state]);

        let owner = |path: &Path| {
            let m = fs::symlink_metadata(path).unwrap();
            (m.uid(), m.gid())
        };
        let before = owner(&outside.join("file"));
        // SAFETY: getuid/getgid have no preconditions.
        let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
        let skipped =
            chown_walk::chown_tree_fd(sec.state_root_fd().unwrap(), &state, uid, gid, None)
                .unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(owner(&state.join("sub/data")), (uid, gid));
        assert_eq!(owner(&outside.join("file")), before);
        assert_eq!(owner(&outside.join("dir/inner")), before);

        // A state directory reached through a service-controlled component is
        // refused instead of opened.
        let svc = b.join("svc");
        fs::create_dir(&svc).unwrap();
        symlink(&state, svc.join("state")).unwrap();
        let trust = Trust::service(UNUSED_UID, UNUSED_UID, None).with_marked(vec![id(&svc)]);
        let err = open_state_root(&svc.join("state"), &trust, false).unwrap_err();
        assert!(
            err.to_string().contains("refusing to use state directory"),
            "{err}"
        );
    }

    #[test]
    fn tls_files_reached_through_the_service_tree_are_not_chowned() {
        let (_b, b) = base();
        let state = b.join("state");
        let etc = b.join("etc");
        fs::create_dir_all(state.join("certs")).unwrap();
        fs::create_dir_all(&etc).unwrap();
        let shadow = b.join("shadow");
        fs::write(&shadow, "secret").unwrap();
        let sec = handle(&state, None, &[&state]);
        // A link the service planted in its tree, as a directory hop.
        symlink(&b, state.join("tls")).unwrap();
        sec.fix_tls_file(&state.join("tls/shadow")).unwrap();
        // An operator link to an in-tree file the service replaced with a link
        // out of the tree: the walk stops at the state root.
        symlink(&shadow, state.join("certs/fullchain.pem")).unwrap();
        symlink(state.join("certs/fullchain.pem"), etc.join("fullchain.pem")).unwrap();
        sec.fix_tls_file(&etc.join("fullchain.pem")).unwrap();
        // A link chain whose second hop is a service-owned directory outside
        // the tree, ending at a file outside it.
        let svc = b.join("svc");
        fs::create_dir(&svc).unwrap();
        symlink(&shadow, svc.join("hop")).unwrap();
        symlink(svc.join("hop"), etc.join("chain.pem")).unwrap();
        let sec2 = handle(&state, None, &[&state, &svc]);
        sec2.fix_tls_file(&etc.join("chain.pem")).unwrap();
        // UNUSED_UID shows that no chown was attempted; the file is untouched.
        assert_eq!(fs::read_to_string(&shadow).unwrap(), "secret");
    }

    #[test]
    fn tls_hard_links_are_chowned_only_when_they_belong_to_the_service() {
        let (_b, b) = base();
        let state = b.join("state");
        fs::create_dir_all(&state).unwrap();
        let cert = b.join("fullchain.pem");
        fs::write(&cert, "x").unwrap();
        fs::hard_link(&cert, b.join("other-name")).unwrap();
        // Owned by the caller, which is neither the service uid nor old_uid:
        // a hard link to another account's file, refused without a chown.
        let sec = handle(&state, None, &[&state]);
        sec.fix_tls_file(&cert).unwrap();
        // Once the other name is gone the file has one link and a chown (to
        // UNUSED_UID) is attempted, which fails for a non-root caller. Files of
        // the old service uid with several links are covered by
        // `chown_walk::tests::decide_skips_links_and_foreign_hard_links_only`.
        fs::remove_file(b.join("other-name")).unwrap();
        // SAFETY: getuid has no preconditions.
        if unsafe { libc::getuid() } != 0 {
            let err = sec.fix_tls_file(&cert).unwrap_err().to_string();
            assert!(err.contains("chown"), "{err}");
        }
    }

    #[test]
    fn writes_into_the_tree_replace_planted_links() {
        let (_b, b) = base();
        let state = b.join("state");
        let etc = b.join("etc");
        fs::create_dir_all(state.join("certs")).unwrap();
        fs::create_dir_all(&etc).unwrap();
        let victim = b.join("victim");
        fs::write(&victim, "keep").unwrap();
        let sec = caller_handle(&state, &[&state]);
        // Operator link /etc/.../fullchain.pem -> <state>/certs/fullchain.pem,
        // which the service replaced with a link to the victim.
        symlink(&victim, state.join("certs/fullchain.pem")).unwrap();
        symlink(state.join("certs/fullchain.pem"), etc.join("fullchain.pem")).unwrap();
        sec.write_file(&etc.join("fullchain.pem"), b"cert", 0o640, Some((0, 0)))
            .unwrap();
        assert_eq!(fs::read_to_string(&victim).unwrap(), "keep");
        assert_eq!(
            fs::read_to_string(state.join("certs/fullchain.pem")).unwrap(),
            "cert"
        );
        assert!(!fs::symlink_metadata(state.join("certs/fullchain.pem"))
            .unwrap()
            .file_type()
            .is_symlink());
        // A planted directory link on the way is refused.
        symlink(&b, state.join("dkim")).unwrap();
        let err = sec
            .write_file(&state.join("dkim/default.txt"), b"x", 0o644, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not followed"), "{err}");
        assert!(!b.join("default.txt").exists());
        // A new file outside the tree in a trusted directory; missing parents
        // are created. The owner is only applied outside the tree, so use
        // none here (a non-root caller cannot give files to root).
        sec.write_file(&etc.join("new/madmail.conf"), b"conf", 0o640, None)
            .unwrap();
        let m = fs::metadata(etc.join("new/madmail.conf")).unwrap();
        assert_eq!(m.permissions().mode() & 0o777, 0o640);
        // create_new_file refuses an existing entry.
        assert!(!sec
            .create_new_file(&etc.join("new/madmail.conf"), b"x", 0o600)
            .unwrap());
        // read_file reads an in-tree file but not a planted link.
        fs::write(state.join("acct.json"), "{}").unwrap();
        assert_eq!(
            sec.read_file(&state.join("acct.json"), 64)
                .unwrap()
                .unwrap(),
            b"{}"
        );
        symlink(&victim, state.join("planted.json")).unwrap();
        assert!(sec.read_file(&state.join("planted.json"), 64).is_err());
        assert!(sec
            .read_file(&state.join("absent.json"), 64)
            .unwrap()
            .is_none());
        // Nor an in-tree file of another account (the caller's, for a service
        // account with another uid): it may be a hard link to a file whose
        // original name was replaced.
        let foreign = handle(&state, None, &[&state]);
        let err = foreign
            .read_file(&state.join("acct.json"), 64)
            .unwrap_err()
            .to_string();
        assert!(err.contains("refusing to read"), "{err}");
        assert!(err.contains("chown madmail:madmail"), "{err}");
    }

    #[test]
    fn a_config_written_outside_the_tree_and_linked_into_it_is_not_given_away() {
        // SAFETY: getuid has no preconditions.
        if unsafe { libc::getuid() } == 0 {
            return;
        }
        let (_b, b) = base();
        let state = b.join("state");
        let etc = b.join("etc");
        fs::create_dir_all(state.join("m1")).unwrap();
        fs::create_dir_all(&etc).unwrap();
        let sec = handle(&state, None, &[&state]);
        let conf = etc.join("madmail.conf");
        sec.write_config_file(&conf, b"ss_password", None).unwrap();
        // The service account links the new config file into its tree.
        fs::hard_link(&conf, state.join("m1/zz-conf")).unwrap();
        let before = fs::metadata(&conf).unwrap();
        // The walk to UNUSED_UID: a chown of the linked config would fail with
        // EPERM on that path. Only the root, changed last, fails.
        let err =
            chown_walk::chown_tree_fd(sec.state_root_fd().unwrap(), &state, sec.uid, sec.gid, None)
                .unwrap_err();
        assert_eq!(err.path, state, "{err:?}");
        let after = fs::metadata(&conf).unwrap();
        assert_eq!((after.uid(), after.gid()), (before.uid(), before.gid()));
        // fix_config_file then refuses the config with two links.
        let err = sec.fix_config_file(&conf).unwrap_err().to_string();
        assert!(err.contains("hard links"), "{err}");
    }

    #[test]
    fn config_outside_the_tree_through_a_service_dir_is_refused() {
        let (_b, b) = base();
        let state = b.join("state");
        fs::create_dir_all(&state).unwrap();
        let svc = b.join("svc-etc");
        fs::create_dir(&svc).unwrap();
        fs::write(svc.join("madmail.conf"), "x").unwrap();
        let sec = handle(&state, None, &[&state, &svc]);
        let err = sec
            .fix_config_file(&svc.join("madmail.conf"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("refusing to change config file"), "{err}");
        // The same file inside the tree, written by install (so owned by the
        // service account), gets mode 0640 through a descriptor.
        let conf = state.join("etc/madmail.conf");
        let owner_sec = caller_handle(&state, &[&state]);
        owner_sec.write_config_file(&conf, b"x", None).unwrap();
        fs::set_permissions(&conf, fs::Permissions::from_mode(0o644)).unwrap();
        owner_sec.fix_config_file(&conf).unwrap();
        assert_eq!(
            fs::metadata(&conf).unwrap().permissions().mode() & 0o777,
            0o640
        );
        // From here the service account has another uid, so the caller's
        // files are another account's.
        let sec = handle(&state, None, &[&state]);
        // A hard link to a foreign file at the config's name is not changed.
        let victim = b.join("victim");
        fs::write(&victim, "x").unwrap();
        fs::set_permissions(&victim, fs::Permissions::from_mode(0o644)).unwrap();
        fs::remove_file(&conf).unwrap();
        fs::hard_link(&victim, &conf).unwrap();
        assert!(sec.fix_config_file(&conf).is_err());
        assert_eq!(
            fs::metadata(&victim).unwrap().permissions().mode() & 0o777,
            0o644
        );
        // So is a single-link in-tree file of another account (here the
        // caller's): it may be a hard link whose original name was replaced.
        fs::remove_file(&conf).unwrap();
        fs::write(&conf, "x").unwrap();
        fs::set_permissions(&conf, fs::Permissions::from_mode(0o644)).unwrap();
        let err = sec.fix_config_file(&conf).unwrap_err().to_string();
        assert!(err.contains("not to the service account"), "{err}");
        assert_eq!(
            fs::metadata(&conf).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }

    #[test]
    fn config_outside_state_dir_that_resolves_into_it_is_refused() {
        let (_b, b) = base();
        let state = b.join("state");
        fs::create_dir_all(state.join("etc")).unwrap();
        fs::write(state.join("etc/madmail.conf"), "x").unwrap();
        let etc = b.join("etc");
        fs::create_dir(&etc).unwrap();
        symlink(state.join("etc"), etc.join("madmail")).unwrap();
        let sec = handle(&state, None, &[&state]);
        let conf = etc.join("madmail/madmail.conf");
        for err in [
            sec.write_config_file(&conf, b"new", None).unwrap_err(),
            sec.fix_config_file(&conf).unwrap_err(),
        ] {
            assert!(err.to_string().contains("resolves into the"), "{err}");
        }
        assert_eq!(
            fs::read_to_string(state.join("etc/madmail.conf")).unwrap(),
            "x"
        );
    }

    #[test]
    fn a_madmail_named_link_to_a_shared_dir_is_left_alone() {
        let (_b, b) = base();
        let state = b.join("state");
        fs::create_dir_all(&state).unwrap();
        let shared = b.join("ssl-private");
        fs::create_dir(&shared).unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o775)).unwrap();
        let etc = b.join("madmail");
        fs::create_dir(&etc).unwrap();
        // /etc/madmail/certs -> /etc/ssl/private, and /etc/madmail -> /etc-like.
        symlink(&shared, etc.join("certs")).unwrap();
        symlink(&shared, b.join("chatmail")).unwrap();
        let sec = handle(&state, None, &[&state]);
        // UNUSED_UID: a chown attempt would fail, so Ok means it was skipped.
        sec.fix_dir(&etc.join("certs")).unwrap();
        sec.fix_dir(&b.join("chatmail")).unwrap();
        assert_eq!(
            fs::metadata(&shared).unwrap().permissions().mode() & 0o777,
            0o775
        );
        // A dedicated directory reached through a link is still handled
        // (the chown to UNUSED_UID then fails for a non-root caller).
        let real = b.join("madmail-real");
        fs::create_dir(&real).unwrap();
        symlink(&real, b.join("madmail-link")).unwrap();
        // SAFETY: getuid has no preconditions.
        if unsafe { libc::getuid() } != 0 {
            let err = sec
                .fix_dir(&b.join("madmail-link"))
                .unwrap_err()
                .to_string();
            assert!(err.contains("chown"), "{err}");
        }
    }

    #[test]
    fn binary_path_inside_or_through_the_state_tree_is_refused() {
        let (_b, b) = base();
        let state = b.join("state");
        fs::create_dir_all(state.join("bin")).unwrap();
        let sec = handle(&state, None, &[&state]);
        let mut cfg = super::super::systemd::tests::sample_cfg();
        cfg.state_dir = state.clone();
        cfg.binary_path = state.join("bin/madmail");
        for check in [
            check_binary_path(&cfg, None),
            check_binary_path(&cfg, Some(&sec)),
        ] {
            let err = check.unwrap_err().to_string();
            assert!(err.contains("inside the state directory"), "{err}");
        }
        // A directory outside --state-dir that is a link into the tree.
        symlink(state.join("bin"), b.join("bin")).unwrap();
        cfg.binary_path = b.join("bin/madmail");
        let err = check_binary_path(&cfg, Some(&sec)).unwrap_err().to_string();
        assert!(err.contains("resolves into the state directory"), "{err}");
        let err = install_binary(&cfg, Some(&sec), false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("refusing to install the binary"), "{err}");
        assert!(!state.join("bin/madmail").exists());
        // A service-controlled directory on the way is refused as well.
        let svc = b.join("svc");
        fs::create_dir(&svc).unwrap();
        let sec = handle(&state, None, &[&state, &svc]);
        cfg.binary_path = svc.join("madmail");
        let err = check_binary_path(&cfg, Some(&sec)).unwrap_err().to_string();
        assert!(err.contains("owned by the service account"), "{err}");
        // A trusted directory: the binary is written through its descriptor,
        // replacing a planted link instead of writing through it.
        let out = b.join("out");
        fs::create_dir(&out).unwrap();
        let victim = b.join("victim");
        fs::write(&victim, "keep").unwrap();
        symlink(&victim, out.join("madmail")).unwrap();
        cfg.binary_path = out.join("madmail");
        // SAFETY: getuid has no preconditions.
        if unsafe { libc::getuid() } != 0 {
            // A non-root caller cannot give the copy to root:root.
            let err = install_binary(&cfg, Some(&sec), false)
                .unwrap_err()
                .to_string();
            assert!(err.contains("install binary"), "{err}");
        } else {
            install_binary(&cfg, Some(&sec), false).unwrap();
            let m = fs::symlink_metadata(out.join("madmail")).unwrap();
            assert!(m.is_file() && m.permissions().mode() & 0o777 == 0o755);
        }
        assert_eq!(fs::read_to_string(&victim).unwrap(), "keep");
        assert!(fs::symlink_metadata(out.join("madmail")).is_ok());
    }

    #[test]
    fn non_dedicated_dirs_are_left_alone() {
        let (_b, b) = base();
        let state = b.join("state");
        fs::create_dir_all(&state).unwrap();
        let shared = b.join("etc");
        fs::create_dir(&shared).unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o775)).unwrap();
        let sec = handle(&state, None, &[&state]);
        // UNUSED_UID: a chown attempt would fail, so Ok means it was skipped.
        sec.fix_dir(&shared).unwrap();
        assert_eq!(
            fs::metadata(&shared).unwrap().permissions().mode() & 0o777,
            0o775
        );
    }

    /// A real directory and its canonical path (macOS tempdirs live behind /var).
    fn real_tempdir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(dir.path()).unwrap();
        (dir, path)
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
