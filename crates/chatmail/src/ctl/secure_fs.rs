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

//! Checked path resolution and descriptor-based writes for code that runs as
//! root next to a service account it does not trust.
//!
//! [`Resolver::resolve`] never hands a path to the kernel. It opens `/` and then
//! each component relative to the previous directory's descriptor with
//! `O_NOFOLLOW`. A symbolic link is read with `readlinkat` and resolution goes
//! on from its target (from `/` for an absolute target), at most
//! [`MAX_LINK_HOPS`] times. Every directory, every link and (when asked) the
//! final object is checked with [`Trust::verdict`] on its own `fstat` /
//! `fstatat(AT_SYMLINK_NOFOLLOW)` data, so a component the service account can
//! change is never followed. Because every directory passed through is trusted,
//! the service account cannot swap an entry between the check and the next
//! `openat`.
//!
//! When the walk reaches the state directory (compared by device and inode), it
//! stops and returns the rest of the path; the caller then works inside the
//! tree with [`tree_open_dir`] and the `*_file` helpers, which open every
//! component with `O_NOFOLLOW` and write by create-new-then-rename, so neither a
//! planted link nor a planted hard link is written through.

use std::ffi::{CStr, CString, OsStr, OsString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

/// Links followed for one path, as for the kernel's own limit (`ELOOP`).
pub const MAX_LINK_HOPS: usize = 40;

/// Directory descriptors used only as a base for `*at` calls. Linux opens them
/// with `O_PATH`; elsewhere a read-only directory open is used.
#[cfg(target_os = "linux")]
const BASE_DIR_FLAGS: libc::c_int =
    libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
#[cfg(not(target_os = "linux"))]
const BASE_DIR_FLAGS: libc::c_int =
    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

/// A directory that is listed, re-owned or re-moded through its descriptor.
pub const DIR_FLAGS: libc::c_int =
    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

/// A regular file opened only to check and change it: no link is followed, a
/// FIFO does not block and a terminal does not become the controlling one.
pub const FILE_FLAGS: libc::c_int =
    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NOCTTY | libc::O_NONBLOCK | libc::O_CLOEXEC;

// ---------------------------------------------------------------------------
// Thin syscall wrappers.

pub fn cstring(s: &OsStr) -> io::Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}

fn check(rc: libc::c_int) -> io::Result<()> {
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// `openat(dir, name, flags, mode)`; `dir` is `AT_FDCWD` for an absolute name.
pub fn openat(dir: RawFd, name: &CStr, flags: libc::c_int, mode: u32) -> io::Result<OwnedFd> {
    // SAFETY: `name` is NUL-terminated and `dir` is AT_FDCWD or an open
    // descriptor owned by the caller for the duration of the call.
    let fd = unsafe { libc::openat(dir, name.as_ptr(), flags, mode as libc::c_uint) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` was just returned by openat and is owned by nobody else.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub fn fstat(fd: RawFd) -> io::Result<libc::stat> {
    // SAFETY: `stat` is plain old data; zero is a valid value to overwrite.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `fd` is open and `st` is writable.
    check(unsafe { libc::fstat(fd, &mut st) })?;
    Ok(st)
}

pub fn fstatat_nofollow(dir: RawFd, name: &CStr) -> io::Result<libc::stat> {
    // SAFETY: `stat` is plain old data; zero is a valid value to overwrite.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `dir` is an open directory descriptor, `name` is NUL-terminated
    // and `st` is writable.
    check(unsafe { libc::fstatat(dir, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) })?;
    Ok(st)
}

/// The target of the link `name` in `dir`.
pub fn readlinkat(dir: RawFd, name: &CStr) -> io::Result<OsString> {
    let mut buf = vec![0u8; 4096];
    loop {
        // SAFETY: `buf` is writable for `buf.len()` bytes; `dir` is open.
        let n = unsafe {
            libc::readlinkat(
                dir,
                name.as_ptr(),
                buf.as_mut_ptr().cast::<libc::c_char>(),
                buf.len(),
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        let n = n as usize;
        if n < buf.len() {
            buf.truncate(n);
            return Ok(OsString::from_vec(buf));
        }
        buf.resize(buf.len() * 2, 0);
    }
}

fn mkdirat(dir: RawFd, name: &CStr, mode: u32) -> io::Result<()> {
    // SAFETY: `dir` is an open directory descriptor and `name` is NUL-terminated.
    check(unsafe { libc::mkdirat(dir, name.as_ptr(), mode as libc::mode_t) })
}

pub fn fchown(fd: RawFd, uid: u32, gid: u32) -> io::Result<()> {
    // SAFETY: `fd` is an open descriptor for the duration of the call.
    check(unsafe { libc::fchown(fd, uid, gid) })
}

pub fn fchmod(fd: RawFd, mode: u32) -> io::Result<()> {
    // SAFETY: `fd` is an open descriptor for the duration of the call.
    check(unsafe { libc::fchmod(fd, mode as libc::mode_t) })
}

fn renameat(dir: RawFd, from: &CStr, to: &CStr) -> io::Result<()> {
    // SAFETY: `dir` is an open directory descriptor; both names are NUL-terminated.
    check(unsafe { libc::renameat(dir, from.as_ptr(), dir, to.as_ptr()) })
}

/// Remove the entry `name` from `dir` without following it (a link is removed
/// itself). `dir_entry` removes an empty directory.
pub fn unlinkat(dir: RawFd, name: &CStr, dir_entry: bool) -> io::Result<()> {
    let flags = if dir_entry { libc::AT_REMOVEDIR } else { 0 };
    // SAFETY: `dir` is an open directory descriptor and `name` is NUL-terminated.
    check(unsafe { libc::unlinkat(dir, name.as_ptr(), flags) })
}

/// File type bits of a `stat`.
pub fn kind(st: &libc::stat) -> libc::mode_t {
    st.st_mode & libc::S_IFMT
}

/// Permission bits (including setuid, setgid and sticky) of a `stat`.
#[allow(clippy::unnecessary_cast)]
pub fn perm_bits(st: &libc::stat) -> u32 {
    (st.st_mode as u32) & 0o7777
}

/// Device and inode of a `stat`, to recognize a directory.
pub type FileId = (u64, u64);

#[allow(clippy::unnecessary_cast)]
pub fn file_id(st: &libc::stat) -> FileId {
    (st.st_dev as u64, st.st_ino as u64)
}

// ---------------------------------------------------------------------------
// Trust rules.

/// Which owners and modes make a component controlled by someone root must not
/// trust.
#[derive(Debug, Clone)]
pub struct Trust {
    uids: Vec<u32>,
    /// The service account's groups (primary and supplementary, never 0): a
    /// directory one of them can write is controlled by the service.
    groups: Vec<u32>,
    root_only: bool,
    /// With `root_only`: one more owner that counts as trusted (the caller, when
    /// not root; used by tests and dry runs).
    also: Option<u32>,
    /// With `root_only`: the well-known directories (by device and inode) that
    /// are trusted although a group other than root can write them, when that
    /// group is not one of `groups` ([`WELL_KNOWN_GROUP_DIRS`]).
    group_dirs: Vec<FileId>,
    /// Tests cannot create files owned by another uid; they mark objects
    /// (by device and inode) that count as owned by the service instead.
    #[cfg(test)]
    marked: Vec<FileId>,
}

fn service_gids(groups: impl IntoIterator<Item = u32>) -> Vec<u32> {
    let mut out = Vec::new();
    for g in groups {
        if g != 0 && !out.contains(&g) {
            out.push(g);
        }
    }
    out
}

impl Trust {
    /// The service account: its uid and, when the account was recreated, the
    /// previous owner of the state directory (`old_uid`), and its primary group
    /// `gid`. Uid and gid 0 are never counted as the service, so root-owned
    /// files stay trusted. Add the supplementary groups with
    /// [`Trust::with_groups`].
    pub fn service(uid: u32, gid: u32, old_uid: Option<u32>) -> Self {
        let mut uids = Vec::new();
        for u in [Some(uid), old_uid].into_iter().flatten() {
            if u != 0 && !uids.contains(&u) {
                uids.push(u);
            }
        }
        Self {
            uids,
            groups: service_gids([gid]),
            root_only: false,
            also: None,
            group_dirs: Vec::new(),
            #[cfg(test)]
            marked: Vec::new(),
        }
    }

    /// Also count directories writable by these groups as the service's.
    pub fn with_groups(mut self, groups: impl IntoIterator<Item = u32>) -> Self {
        self.groups = service_gids(self.groups.iter().copied().chain(groups));
        self
    }

    /// Count the objects with these ids as the service account's (tests).
    #[cfg(test)]
    pub fn with_marked(mut self, ids: Vec<FileId>) -> Self {
        self.marked = ids;
        self
    }

    /// Stricter rule for uninstall and staging: anything not owned by root is
    /// untrusted, and so is anything a group other than root can write: a file,
    /// and a directory without the sticky bit. A sticky directory is handled
    /// like `/tmp`: every component must still be owned by root, so nobody else
    /// can rename or replace the entries followed in it. `service_groups` are
    /// the groups of the service accounts. The only group-writable directories
    /// without the sticky bit that pass are the well-known ones in
    /// [`WELL_KNOWN_GROUP_DIRS`] (Ubuntu's `/var/log`, `root:syslog` 0775, and
    /// Debian's `/var/mail`, `root:mail` 2775), recognized by the device and
    /// inode of the directory each path resolves to, and only when their group
    /// is not one of `service_groups`.
    pub fn root_only(service_groups: impl IntoIterator<Item = u32>) -> Self {
        use std::os::unix::fs::MetadataExt;
        let group_dirs = WELL_KNOWN_GROUP_DIRS
            .iter()
            .filter_map(|d| std::fs::metadata(d).ok())
            .filter(|m| m.is_dir())
            .map(|m| (m.dev(), m.ino()))
            .collect();
        Self {
            uids: Vec::new(),
            groups: service_gids(service_groups),
            root_only: true,
            also: None,
            group_dirs,
            #[cfg(test)]
            marked: Vec::new(),
        }
    }

    /// Trust no well-known group-writable directory ([`WELL_KNOWN_GROUP_DIRS`]),
    /// for when the service accounts' groups may be incomplete.
    pub fn without_group_dirs(mut self) -> Self {
        self.group_dirs.clear();
        self
    }

    /// Count these directories (by device and inode) as well-known
    /// group-writable directories (tests).
    #[cfg(test)]
    pub fn with_group_dirs(mut self, ids: Vec<FileId>) -> Self {
        self.group_dirs = ids;
        self
    }

    /// [`Trust::root_only`] for the effective user: when it is not root, its
    /// own files count as trusted too, since it can change them anyway.
    pub fn root_or_caller(service_groups: impl IntoIterator<Item = u32>) -> Self {
        // SAFETY: geteuid has no preconditions.
        let euid = unsafe { libc::geteuid() };
        Self {
            also: (euid != 0).then_some(euid),
            ..Self::root_only(service_groups)
        }
    }

    /// Why a component with this `stat` must not be followed or changed by
    /// path, or `None` when it is trusted. `/` and root-owned directories with
    /// mode 0755 pass. Link permission bits carry no meaning and are ignored.
    ///
    /// Only the mode bits are read: a POSIX ACL entry that lets the service
    /// account write a directory is seen only as group write permission (the
    /// ACL mask), so it is caught only when the directory's group is one of the
    /// service account's groups.
    pub fn verdict(&self, st: &libc::stat) -> Option<&'static str> {
        if self.root_only && st.st_uid != 0 && Some(st.st_uid) != self.also {
            return Some("not owned by root");
        }
        #[cfg(test)]
        if self.marked.contains(&file_id(st)) {
            return Some("owned by the service account");
        }
        if self.uids.contains(&st.st_uid) {
            return Some("owned by the service account");
        }
        let mode = perm_bits(st);
        let group_writable = mode & 0o020 != 0;
        match kind(st) {
            libc::S_IFLNK => None,
            libc::S_IFDIR => {
                let sticky = mode & 0o1000 != 0;
                if group_writable && self.groups.contains(&st.st_gid) {
                    Some("group-writable by a group of the service account")
                } else if mode & 0o002 != 0 && !sticky {
                    Some("world-writable without the sticky bit")
                } else if group_writable
                    && self.root_only
                    && st.st_gid != 0
                    && !sticky
                    && !self.group_dirs.contains(&file_id(st))
                {
                    Some("group-writable by a group other than root, without the sticky bit")
                } else {
                    None
                }
            }
            _ if group_writable && self.root_only && st.st_gid != 0 => {
                Some("group-writable by a group other than root")
            }
            _ if mode & 0o002 != 0 => Some("world-writable"),
            _ => None,
        }
    }
}

/// Directories that [`Trust::root_only`] trusts although a group other than
/// root can write them (see there).
pub const WELL_KNOWN_GROUP_DIRS: &[&str] = &["/var/log", "/var/mail"];

/// Whether a passwd lookup error code means "no such entry". Only `ENOENT`
/// does (musl reports a missing `/etc/passwd` with it); glibc and musl report a
/// missing entry with 0 and a NULL result. Every other code, including the
/// `ESRCH`, `EBADF` and `EPERM` that POSIX also allows for "not found", is an
/// error: an NSS module can return them for an account that exists, and a
/// caller that took that account for one without a name could give its files
/// to the service account.
fn not_found_code(rc: libc::c_int) -> bool {
    rc == libc::ENOENT
}

/// Run a passwd lookup (`getpwuid_r` or `getpwnam_r`, given as `call`) with a
/// buffer that grows on `ERANGE`, and give the entry found to `read`. `Ok(None)`
/// when there is no entry; an error when the account database cannot be read
/// (for example an NSS service that is down), so a caller can fail closed.
fn passwd_lookup<T>(
    call: impl Fn(&mut libc::passwd, &mut [libc::c_char], &mut *mut libc::passwd) -> libc::c_int,
    read: impl FnOnce(&libc::passwd) -> T,
) -> io::Result<Option<T>> {
    let mut buf = vec![0 as libc::c_char; 1024];
    loop {
        // SAFETY: `passwd` is plain old data; zero is a valid value to overwrite.
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let rc = call(&mut pwd, &mut buf, &mut result);
        if rc == libc::ERANGE && buf.len() < (1 << 20) {
            let len = buf.len() * 2;
            buf.resize(len, 0);
            continue;
        }
        return match rc {
            0 if result.is_null() => Ok(None),
            0 => Ok(Some(read(&pwd))),
            rc if not_found_code(rc) => Ok(None),
            rc => Err(io::Error::from_raw_os_error(rc)),
        };
    }
}

/// What `getent passwd <key>` says, from its exit status (`None` when it was
/// killed by a signal) and its output: `Ok(Some(field))` with the `field`-th
/// (0-based) `:`-separated field of the entry for exit status 0, `Ok(None)`
/// for exit status 2 ("no such entry"), an error for anything else.
fn parse_getent(
    code: Option<i32>,
    stdout: &[u8],
    field: usize,
    key: &str,
) -> io::Result<Option<String>> {
    match code {
        Some(0) => {
            let text = String::from_utf8_lossy(stdout);
            let line = text.lines().next().unwrap_or_default();
            match line.split(':').nth(field) {
                Some(value) if line.split(':').count() >= 7 => Ok(Some(value.to_string())),
                _ => Err(io::Error::other(format!(
                    "getent passwd {key} printed an entry that cannot be read"
                ))),
            }
        }
        Some(2) => Ok(None),
        Some(code) => Err(io::Error::other(format!(
            "getent passwd {key} failed with exit status {code}"
        ))),
        None => Err(io::Error::other(format!(
            "getent passwd {key} was killed by a signal"
        ))),
    }
}

/// Ask the host's account database (`getent passwd <key>`, which uses the
/// host's NSS configuration) for field `field` of the entry for `key`, after
/// the C library found none. A statically linked musl build reads only
/// `/etc/passwd` and never asks NSS modules (LDAP, sssd, winbind,
/// systemd-userdb), so an account that exists only there would otherwise look
/// like a uid without a name. `Ok(None)` when `getent` finds no entry or is
/// not installed (a host without it, such as BusyBox, has no NSS modules
/// either); an error when it fails in any other way, so callers fail closed.
/// An all-digit `key` is looked up as a uid. `getent` cannot tell an NSS
/// service that is down from an entry that does not exist: both exit with 2.
fn getent_passwd(key: &str, field: usize) -> io::Result<Option<String>> {
    match std::process::Command::new("getent")
        .arg("passwd")
        .arg(key)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(out) => parse_getent(out.status.code(), &out.stdout, field, key),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io::Error::other(format!(
            "cannot run getent passwd {key}: {e}"
        ))),
    }
}

/// The name of the account with uid `uid` (`getpwuid_r`, then
/// [`getent_passwd`] when that finds none): `Ok(None)` when no account has
/// that uid, an error when the account database cannot be read.
pub fn account_name(uid: u32) -> io::Result<Option<String>> {
    account_name_c(uid)
        .and_then(|found| match found {
            Some(name) => Ok(Some(name)),
            None => getent_passwd(&uid.to_string(), 0),
        })
        .map_err(|e| io::Error::new(e.kind(), format!("cannot look up uid {uid}: {e}")))
}

/// [`account_name`] from the C library alone.
fn account_name_c(uid: u32) -> io::Result<Option<String>> {
    passwd_lookup(
        |pwd, buf, result| {
            // SAFETY: `pwd`, `buf` and `result` are valid for writes, and
            // `buf.len()` is the buffer's size.
            unsafe {
                libc::getpwuid_r(uid as libc::uid_t, pwd, buf.as_mut_ptr(), buf.len(), result)
            }
        },
        // SAFETY: a successful lookup sets pw_name to a NUL-terminated string
        // in `buf`, which is still alive here.
        |pwd| {
            unsafe { CStr::from_ptr(pwd.pw_name) }
                .to_string_lossy()
                .into_owned()
        },
    )
}

/// The primary gid of account `user` (`getpwnam_r`, then [`getent_passwd`]
/// when that finds none): `Ok(None)` when there is no such account, an error
/// when the account database cannot be read.
fn account_gid(user: &str) -> io::Result<Option<u32>> {
    account_gid_c(user)
        .and_then(|found| match found {
            Some(gid) => Ok(Some(gid)),
            // `getent` takes an all-digit key for a uid; such a name is not
            // asked for (useradd refuses one).
            None if user.bytes().all(|b| b.is_ascii_digit()) => Ok(None),
            None => match getent_passwd(user, 3)? {
                Some(gid) => gid.parse().map(Some).map_err(|_| {
                    io::Error::other(format!("getent passwd {user} printed gid {gid:?}"))
                }),
                None => Ok(None),
            },
        })
        .map_err(|e| io::Error::new(e.kind(), format!("cannot look up account {user}: {e}")))
}

/// [`account_gid`] from the C library alone.
fn account_gid_c(user: &str) -> io::Result<Option<u32>> {
    let name = CString::new(user).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    passwd_lookup(
        |pwd, buf, result| {
            // SAFETY: `name` is NUL-terminated; `pwd`, `buf` and `result` are
            // valid for writes, and `buf.len()` is the buffer's size.
            unsafe { libc::getpwnam_r(name.as_ptr(), pwd, buf.as_mut_ptr(), buf.len(), result) }
        },
        |pwd| pwd.pw_gid,
    )
}

/// The groups of account `user` (primary and supplementary): `Ok(None)` when
/// the account does not exist ([`account_gid`]), an error when it exists but its
/// groups cannot be read (`id -G`) or the account database cannot be read.
/// Callers fail closed on an error: a group they do not know of could write a
/// directory they then trust.
pub fn account_groups_checked(user: &str) -> io::Result<Option<Vec<u32>>> {
    let Some(primary) = account_gid(user)? else {
        return Ok(None);
    };
    let out = std::process::Command::new("id")
        .arg("-G")
        .arg(user)
        .output()
        .map_err(|e| io::Error::other(format!("cannot read the groups of account {user}: {e}")))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let groups: Option<Vec<u32>> = text.split_whitespace().map(|g| g.parse().ok()).collect();
    match groups {
        Some(mut groups) if out.status.success() && !groups.is_empty() => {
            if !groups.contains(&primary) {
                groups.push(primary);
            }
            Ok(Some(groups))
        }
        _ => Err(io::Error::other(format!(
            "cannot read the groups of account {user}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Resolution.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
    Dir,
    File,
}

/// An object opened by [`Resolver::resolve`]: a directory opened with
/// [`DIR_FLAGS`] or a regular file opened with [`FILE_FLAGS`].
#[derive(Debug)]
pub struct Opened {
    pub fd: OwnedFd,
    pub st: libc::stat,
    /// The physical path walked to reach it (links replaced by their targets).
    pub path: PathBuf,
}

#[derive(Debug)]
pub enum Resolved {
    /// Every component passed the trust check; the object is open.
    Found(Opened),
    /// The walk reached the state directory; the rest of the path, relative to
    /// it, was not resolved (see [`tree_open_dir`]).
    InTree(PathBuf),
    /// A component failed the trust check; nothing after it was looked at.
    Controlled {
        component: PathBuf,
        reason: &'static str,
    },
    /// `name` does not exist in the trusted directory `parent`; `rest` is what
    /// would follow it.
    Missing {
        parent: Opened,
        name: OsString,
        rest: PathBuf,
    },
}

pub struct Resolver<'a> {
    pub trust: &'a Trust,
    /// Device and inode of the state directory, if any.
    pub state_root: Option<FileId>,
    /// Also apply the trust check to the final object.
    pub check_final: bool,
}

fn rest_path(pending: &[OsString]) -> PathBuf {
    pending.iter().rev().collect()
}

fn push_components(pending: &mut Vec<OsString>, path: &Path) {
    let mut parts = Vec::new();
    for c in path.components() {
        match c {
            Component::Normal(n) => parts.push(n.to_os_string()),
            Component::ParentDir => parts.push(OsString::from("..")),
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    pending.extend(parts.into_iter().rev());
}

fn errno(code: i32) -> io::Error {
    io::Error::from_raw_os_error(code)
}

impl Resolver<'_> {
    /// Resolve the absolute `path` component by component (see the module docs).
    pub fn resolve(&self, path: &Path, want: Want) -> io::Result<Resolved> {
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{} is not an absolute path",
                    crate::ctl::host_paths::shown(path)
                ),
            ));
        }
        let root_fd = openat(libc::AT_FDCWD, c"/", BASE_DIR_FLAGS, 0)?;
        let root_st = fstat(root_fd.as_raw_fd())?;
        let root = Opened {
            fd: root_fd,
            st: root_st,
            path: PathBuf::from("/"),
        };
        let mut pending = Vec::new();
        push_components(&mut pending, path);
        if let Some(found) = self.check_dir(&root, &pending, pending.is_empty())? {
            return Ok(found);
        }
        let mut stack = vec![root];
        let mut hops = 0;
        loop {
            let Some(name) = pending.pop() else {
                // The path ends in a directory already walked (`/` or `..`).
                let top = stack.pop().expect("stack holds `/`");
                if want == Want::File {
                    return Err(errno(libc::EISDIR));
                }
                let fd = openat(top.fd.as_raw_fd(), c".", DIR_FLAGS, 0)?;
                let st = fstat(fd.as_raw_fd())?;
                return Ok(Resolved::Found(Opened {
                    fd,
                    st,
                    path: top.path,
                }));
            };
            if name == ".." {
                if stack.len() > 1 {
                    stack.pop();
                }
                continue;
            }
            let is_last = pending.is_empty();
            let cur = stack.last().expect("stack holds `/`");
            let c_name = cstring(&name)?;
            let here = cur.path.join(&name);
            let st = match fstatat_nofollow(cur.fd.as_raw_fd(), &c_name) {
                Ok(st) => st,
                Err(e) if e.raw_os_error() == Some(libc::ENOENT) => {
                    let parent = stack.pop().expect("stack holds `/`");
                    return Ok(Resolved::Missing {
                        parent,
                        name,
                        rest: rest_path(&pending),
                    });
                }
                Err(e) => return Err(e),
            };
            match kind(&st) {
                libc::S_IFLNK => {
                    if let Some(reason) = self.trust.verdict(&st) {
                        return Ok(Resolved::Controlled {
                            component: here,
                            reason,
                        });
                    }
                    hops += 1;
                    if hops > MAX_LINK_HOPS {
                        return Err(errno(libc::ELOOP));
                    }
                    let target = PathBuf::from(readlinkat(cur.fd.as_raw_fd(), &c_name)?);
                    if target.is_absolute() {
                        stack.truncate(1);
                    }
                    push_components(&mut pending, &target);
                }
                libc::S_IFDIR if !is_last || want == Want::Dir => {
                    let flags = if is_last { DIR_FLAGS } else { BASE_DIR_FLAGS };
                    let fd = openat(cur.fd.as_raw_fd(), &c_name, flags, 0)?;
                    let st = fstat(fd.as_raw_fd())?;
                    let opened = Opened { fd, st, path: here };
                    if let Some(done) = self.check_dir(&opened, &pending, is_last)? {
                        return Ok(done);
                    }
                    if is_last {
                        return Ok(Resolved::Found(opened));
                    }
                    stack.push(opened);
                }
                _ if !is_last => return Err(errno(libc::ENOTDIR)),
                libc::S_IFDIR => return Err(errno(libc::EISDIR)),
                libc::S_IFREG if want == Want::File => {
                    let fd = openat(cur.fd.as_raw_fd(), &c_name, FILE_FLAGS, 0)?;
                    let st = fstat(fd.as_raw_fd())?;
                    if kind(&st) != libc::S_IFREG {
                        return Err(errno(libc::EINVAL));
                    }
                    if self.check_final {
                        if let Some(reason) = self.trust.verdict(&st) {
                            return Ok(Resolved::Controlled {
                                component: here,
                                reason,
                            });
                        }
                    }
                    return Ok(Resolved::Found(Opened { fd, st, path: here }));
                }
                libc::S_IFREG => return Err(errno(libc::ENOTDIR)),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "{} is not a regular file or directory",
                            crate::ctl::host_paths::shown(&here)
                        ),
                    ))
                }
            }
        }
    }

    /// The state-root and trust checks for an opened directory.
    fn check_dir(
        &self,
        dir: &Opened,
        pending: &[OsString],
        is_last: bool,
    ) -> io::Result<Option<Resolved>> {
        if self.state_root == Some(file_id(&dir.st)) {
            return Ok(Some(Resolved::InTree(rest_path(pending))));
        }
        if !is_last || self.check_final {
            if let Some(reason) = self.trust.verdict(&dir.st) {
                return Ok(Some(Resolved::Controlled {
                    component: dir.path.clone(),
                    reason,
                }));
            }
        }
        Ok(None)
    }

    /// Resolve `path` as a directory, creating missing components (mode 0755)
    /// in trusted directories, as `mkdir -p` would.
    pub fn ensure_dir(&self, path: &Path) -> io::Result<Resolved> {
        for _ in 0..4096 {
            match self.resolve(path, Want::Dir)? {
                Resolved::Missing { parent, name, .. } => {
                    match mkdirat(parent.fd.as_raw_fd(), &cstring(&name)?, 0o755) {
                        Ok(()) => {}
                        Err(e) if e.raw_os_error() == Some(libc::EEXIST) => {}
                        Err(e) => return Err(e),
                    }
                }
                other => return Ok(other),
            }
        }
        Err(errno(libc::ELOOP))
    }
}

// ---------------------------------------------------------------------------
// Work inside the state tree, below an opened state-root descriptor.

/// Open the directory `rel` below `root` with `O_NOFOLLOW | O_DIRECTORY` at
/// every component, creating missing ones (mode 0755) when `create` is set. A
/// link or a non-directory on the way fails with `ELOOP` or `ENOTDIR`; `..` is
/// refused.
pub fn tree_open_dir(root: &OwnedFd, rel: &Path, create: bool) -> io::Result<OwnedFd> {
    let mut cur = openat(root.as_raw_fd(), c".", DIR_FLAGS, 0)?;
    for c in rel.components() {
        let name = match c {
            Component::Normal(n) => cstring(n)?,
            Component::CurDir => continue,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "{} leaves the state directory",
                        crate::ctl::host_paths::shown(rel)
                    ),
                ))
            }
        };
        let next = match openat(cur.as_raw_fd(), &name, DIR_FLAGS, 0) {
            Err(e) if create && e.raw_os_error() == Some(libc::ENOENT) => {
                match mkdirat(cur.as_raw_fd(), &name, 0o755) {
                    Ok(()) => {}
                    Err(e) if e.raw_os_error() == Some(libc::EEXIST) => {}
                    Err(e) => return Err(e),
                }
                openat(cur.as_raw_fd(), &name, DIR_FLAGS, 0)?
            }
            other => other?,
        };
        cur = next;
    }
    Ok(cur)
}

/// Split a relative file path into its directory and its last component.
pub fn split_file(rel: &Path) -> io::Result<(PathBuf, OsString)> {
    match rel.components().next_back() {
        Some(Component::Normal(name)) => Ok((
            rel.parent().map(Path::to_path_buf).unwrap_or_default(),
            name.to_os_string(),
        )),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} does not name a file",
                crate::ctl::host_paths::shown(rel)
            ),
        )),
    }
}

fn temp_name(name: &OsStr) -> io::Result<CString> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    let suffix: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let mut tmp = OsString::from(".");
    tmp.push(name);
    tmp.push(format!(".tmp-{suffix}"));
    cstring(&tmp)
}

fn write_all(fd: RawFd, mut data: &[u8]) -> io::Result<()> {
    while !data.is_empty() {
        // SAFETY: `fd` is open for writing and `data` is readable.
        let n = unsafe { libc::write(fd, data.as_ptr().cast(), data.len()) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        data = &data[n as usize..];
    }
    Ok(())
}

/// Create `name` in `dir` with `data`, replacing any existing entry: a new file
/// is created under a temporary name with `O_CREAT | O_EXCL | O_NOFOLLOW`,
/// given `mode` (and `owner`) through its descriptor, written, and renamed
/// over `name`. The previous entry, whatever it was (a link, a hard link to
/// another file), is unlinked by the rename and never opened or written.
/// Returns the new file's `stat`.
pub fn replace_file(
    dir: RawFd,
    name: &OsStr,
    data: &[u8],
    mode: u32,
    owner: Option<(u32, u32)>,
) -> io::Result<libc::stat> {
    replace_file_with(dir, name, mode, owner, |fd| write_all(fd, data))
}

/// [`replace_file`] with the content written by `fill` on the new file's
/// descriptor (which is then synced).
pub fn replace_file_with(
    dir: RawFd,
    name: &OsStr,
    mode: u32,
    owner: Option<(u32, u32)>,
    fill: impl FnOnce(RawFd) -> io::Result<()>,
) -> io::Result<libc::stat> {
    let target = cstring(name)?;
    let tmp = temp_name(name)?;
    let fd = openat(
        dir,
        &tmp,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        mode,
    )?;
    let result = (|| {
        fchmod(fd.as_raw_fd(), mode)?;
        if let Some((uid, gid)) = owner {
            fchown(fd.as_raw_fd(), uid, gid)?;
        }
        fill(fd.as_raw_fd())?;
        // SAFETY: `fd` is open.
        check(unsafe { libc::fsync(fd.as_raw_fd()) })?;
        let st = fstat(fd.as_raw_fd())?;
        renameat(dir, &tmp, &target)?;
        Ok(st)
    })();
    if result.is_err() {
        let _ = unlinkat(dir, &tmp, false);
    }
    result
}

/// Create `name` in `dir` only if it does not exist (`O_CREAT | O_EXCL |
/// O_NOFOLLOW`), with `mode` (and `owner`) set through the descriptor before
/// anything is written. Returns the new file's `stat`, or `None` when an entry
/// of that name (of any kind) already exists.
pub fn create_new_file(
    dir: RawFd,
    name: &OsStr,
    data: &[u8],
    mode: u32,
    owner: Option<(u32, u32)>,
) -> io::Result<Option<libc::stat>> {
    let c_name = cstring(name)?;
    let fd = match openat(
        dir,
        &c_name,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        mode,
    ) {
        Ok(fd) => fd,
        Err(e) if e.raw_os_error() == Some(libc::EEXIST) => return Ok(None),
        Err(e) => return Err(e),
    };
    let result = (|| {
        fchmod(fd.as_raw_fd(), mode)?;
        if let Some((uid, gid)) = owner {
            fchown(fd.as_raw_fd(), uid, gid)?;
        }
        write_all(fd.as_raw_fd(), data)?;
        // SAFETY: `fd` is open.
        check(unsafe { libc::fsync(fd.as_raw_fd()) })?;
        fstat(fd.as_raw_fd())
    })();
    if result.is_err() {
        let _ = unlinkat(dir, &c_name, false);
    }
    result.map(Some)
}

fn not_regular() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "not a regular file")
}

/// Open `name` in `dir` read-only as a regular file without following a
/// link; `None` when it does not exist. A link, a directory or a special file
/// is an error.
///
/// On Linux the entry is first opened with `O_PATH | O_NOFOLLOW`, which never
/// opens the object itself, and checked with `fstat` on that descriptor; only
/// a regular file is then opened for reading, through `/proc/self/fd`, which
/// reopens the same inode (checked again by device and inode). So a device
/// node the service account swapped in or hard-linked there is never opened.
/// Elsewhere the entry is checked with `fstatat` and opened with
/// [`FILE_FLAGS`] (`O_NONBLOCK | O_NOCTTY`); a device swapped in between
/// those two calls would be opened, and is then refused by the `fstat` on
/// the descriptor.
pub fn open_regular_file(dir: RawFd, name: &OsStr) -> io::Result<Option<(OwnedFd, libc::stat)>> {
    let c_name = cstring(name)?;
    #[cfg(target_os = "linux")]
    {
        let path_fd = match openat(
            dir,
            &c_name,
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0,
        ) {
            Ok(fd) => fd,
            Err(e) if e.raw_os_error() == Some(libc::ENOENT) => return Ok(None),
            Err(e) => return Err(e),
        };
        let st = fstat(path_fd.as_raw_fd())?;
        if kind(&st) != libc::S_IFREG {
            return Err(not_regular());
        }
        let proc_path = CString::new(format!("/proc/self/fd/{}", path_fd.as_raw_fd()))
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // The /proc entry is a link to the inode itself, so O_NOFOLLOW is not
        // used here.
        // Without /proc there is no way to reopen the O_PATH descriptor, and
        // reopening by name would follow what the service account put there,
        // so this is an error, not a fallback.
        let fd = openat(
            libc::AT_FDCWD,
            &proc_path,
            libc::O_RDONLY | libc::O_NOCTTY | libc::O_NONBLOCK | libc::O_CLOEXEC,
            0,
        )
        .map_err(|e| match e.raw_os_error() {
            Some(libc::ENOENT) | Some(libc::ENOTDIR) => io::Error::new(
                e.kind(),
                format!(
                    "cannot reopen the file through /proc/self/fd ({e}): /proc must be \
                     mounted for install to read or change files inside the state directory"
                ),
            ),
            _ => e,
        })?;
        let again = fstat(fd.as_raw_fd())?;
        if file_id(&again) != file_id(&st) || kind(&again) != libc::S_IFREG {
            return Err(not_regular());
        }
        Ok(Some((fd, again)))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let st = match fstatat_nofollow(dir, &c_name) {
            Ok(st) => st,
            Err(e) if e.raw_os_error() == Some(libc::ENOENT) => return Ok(None),
            Err(e) => return Err(e),
        };
        if kind(&st) != libc::S_IFREG {
            return Err(not_regular());
        }
        let fd = openat(dir, &c_name, FILE_FLAGS, 0)?;
        let st = fstat(fd.as_raw_fd())?;
        if kind(&st) != libc::S_IFREG {
            return Err(not_regular());
        }
        Ok(Some((fd, st)))
    }
}

/// [`open_regular_file`] that also refuses a file with more than one hard
/// link: the service account could have linked another file there.
pub fn open_single_link_file(
    dir: RawFd,
    name: &OsStr,
) -> io::Result<Option<(OwnedFd, libc::stat)>> {
    let Some((fd, st)) = open_regular_file(dir, name)? else {
        return Ok(None);
    };
    if st.st_nlink > 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("has {} hard links", st.st_nlink),
        ));
    }
    Ok(Some((fd, st)))
}

/// Read a file opened by [`open_single_link_file`], up to `max` bytes.
pub fn read_fd(fd: &OwnedFd, max: usize) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let dup = fd.try_clone()?;
    let mut file = std::fs::File::from(dup);
    let mut out = Vec::new();
    file.by_ref().take(max as u64 + 1).read_to_end(&mut out)?;
    if out.len() > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "file too large"));
    }
    Ok(out)
}

/// List the entries of the open directory `fd`, except `.` and `..`. The
/// listing uses a duplicate descriptor, so `fd` stays open for `*at` calls.
pub fn read_names(fd: &OwnedFd) -> io::Result<Vec<CString>> {
    // SAFETY: `fd` is an open descriptor; dup returns a new one or -1.
    let dup = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    if dup < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `dup` is a fresh descriptor. On success fdopendir takes ownership
    // of it (closedir closes it); on failure it stays ours and is closed here.
    let dirp = unsafe { libc::fdopendir(dup) };
    if dirp.is_null() {
        let e = io::Error::last_os_error();
        // SAFETY: fdopendir failed, so `dup` is still owned by us.
        drop(unsafe { OwnedFd::from_raw_fd(dup) });
        return Err(e);
    }
    let stream = DirStream(dirp);
    let mut names = Vec::new();
    loop {
        clear_errno();
        // SAFETY: `stream.0` is a valid DIR stream until DirStream is dropped.
        let entry = unsafe { libc::readdir(stream.0) };
        if entry.is_null() {
            // NULL is the end of the stream unless errno was set.
            let e = io::Error::last_os_error();
            return match e.raw_os_error() {
                Some(0) | None => Ok(names),
                Some(_) => Err(e),
            };
        }
        // SAFETY: readdir returned a valid dirent whose d_name is NUL-terminated;
        // the name is copied before the next readdir call. d_name's declared
        // length differs between platforms, so it is read as a C string only.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            names.push(name.to_owned());
        }
    }
}

/// Owns a `DIR*` from fdopendir and closes it (and its descriptor) on drop.
struct DirStream(*mut libc::DIR);

impl Drop for DirStream {
    fn drop(&mut self) {
        // SAFETY: the pointer came from a successful fdopendir and is closed once.
        unsafe {
            libc::closedir(self.0);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "dragonfly"))]
fn clear_errno() {
    // SAFETY: __errno_location returns this thread's errno slot.
    unsafe { *libc::__errno_location() = 0 }
}

#[cfg(any(target_vendor = "apple", target_os = "freebsd"))]
fn clear_errno() {
    // SAFETY: __error returns this thread's errno slot.
    unsafe { *libc::__error() = 0 }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "dragonfly",
    target_vendor = "apple",
    target_os = "freebsd"
)))]
fn clear_errno() {
    // Other Unix targets are not built; a NULL readdir is then read as the end
    // of the stream only when errno happens to be zero.
}

/// Directory levels [`remove_dir_tree`] descends; each open level holds one
/// descriptor.
pub const MAX_REMOVE_DEPTH: usize = 256;

/// One directory being emptied by [`remove_dir_tree`].
struct RemoveLevel {
    fd: OwnedFd,
    /// The directory's name in its parent (the level below on the stack).
    name: CString,
    names: std::vec::IntoIter<CString>,
    /// How often the directory was listed again after its entries were removed.
    relisted: usize,
}

/// Remove the directory `name` in the open directory `dir`, with everything
/// in it, through descriptors only: `expected` is the device and inode the
/// caller checked (`fstatat` without following), and the directory is opened
/// with `O_NOFOLLOW | O_DIRECTORY` and compared with it. Each directory below
/// is opened relative to its parent's descriptor the same way and listed with
/// `fdopendir`; every other entry (a link too) is removed with `unlinkat` on
/// the parent's descriptor, so nothing is followed and no path is resolved
/// again. An entry that another process replaces is looked at again; one that
/// disappears is skipped. A directory that still has entries after being
/// listed eight more times, or a tree deeper than [`MAX_REMOVE_DEPTH`], is an
/// error.
pub fn remove_dir_tree(dir: RawFd, name: &CStr, expected: FileId) -> io::Result<()> {
    let fd = openat(dir, name, DIR_FLAGS, 0)?;
    if file_id(&fstat(fd.as_raw_fd())?) != expected {
        return Err(io::Error::other(
            "the directory was replaced after it was checked",
        ));
    }
    let names = read_names(&fd)?;
    let mut stack = vec![RemoveLevel {
        fd,
        name: name.to_owned(),
        names: names.into_iter(),
        relisted: 0,
    }];
    while let Some(top) = stack.last_mut() {
        let Some(entry) = top.names.next() else {
            let again = read_names(&top.fd)?;
            if !again.is_empty() && top.relisted < 8 {
                top.relisted += 1;
                top.names = again.into_iter();
                continue;
            }
            let done = stack.pop().expect("stack is not empty");
            let parent = stack.last().map_or(dir, |l| l.fd.as_raw_fd());
            drop(done.fd);
            match unlinkat(parent, &done.name, true) {
                Err(e) if e.raw_os_error() == Some(libc::ENOENT) && !stack.is_empty() => {}
                other => other?,
            }
            continue;
        };
        let parent = top.fd.as_raw_fd();
        if let Some(child) = remove_entry_at(parent, &entry, stack.len())? {
            stack.push(child);
        }
    }
    Ok(())
}

/// Remove one entry of a directory being emptied, or open it when it is a
/// directory (returned to be emptied next). `depth` is the number of open
/// levels.
fn remove_entry_at(parent: RawFd, name: &CStr, depth: usize) -> io::Result<Option<RemoveLevel>> {
    let gone = |e: &io::Error| e.raw_os_error() == Some(libc::ENOENT);
    for _ in 0..8 {
        let st = match fstatat_nofollow(parent, name) {
            Ok(st) => st,
            Err(e) if gone(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        if kind(&st) == libc::S_IFDIR {
            if depth >= MAX_REMOVE_DEPTH {
                return Err(io::Error::other(format!(
                    "deeper than {MAX_REMOVE_DEPTH} levels"
                )));
            }
            match openat(parent, name, DIR_FLAGS, 0) {
                Ok(fd) => {
                    let names = read_names(&fd)?;
                    return Ok(Some(RemoveLevel {
                        fd,
                        name: name.to_owned(),
                        names: names.into_iter(),
                        relisted: 0,
                    }));
                }
                Err(e) if gone(&e) => return Ok(None),
                // Replaced by a link or a file since the fstatat: look again.
                Err(e) if matches!(e.raw_os_error(), Some(libc::ELOOP) | Some(libc::ENOTDIR)) => {}
                Err(e) => return Err(e),
            }
        } else {
            match unlinkat(parent, name, false) {
                Ok(()) => return Ok(None),
                Err(e) if gone(&e) => return Ok(None),
                // Replaced by a directory since the fstatat: look again.
                Err(e) if matches!(e.raw_os_error(), Some(libc::EISDIR) | Some(libc::EPERM)) => {}
                Err(e) => return Err(e),
            }
        }
    }
    Err(io::Error::other(
        "the entry kept changing while it was removed",
    ))
}

// ---------------------------------------------------------------------------
// Running a step as the service account.

/// While alive, the process runs with the service account's effective uid,
/// gid and groups (only `gid`), so files it creates belong to the service and
/// links it follows reach only what the service could reach itself. The C
/// library applies these calls to every thread of the process.
pub struct AsService {
    groups: Vec<libc::gid_t>,
    restored: bool,
}

impl AsService {
    pub fn enter(uid: u32, gid: u32) -> io::Result<Self> {
        // SAFETY: getgroups with a zero size only returns the count.
        let n = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut groups = vec![0 as libc::gid_t; n as usize];
        // SAFETY: `groups` has room for `n` entries.
        let n = unsafe { libc::getgroups(n, groups.as_mut_ptr()) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        groups.truncate(n as usize);
        let guard = Self {
            groups,
            restored: false,
        };
        let one = [gid as libc::gid_t];
        // SAFETY: `one` holds one valid gid.
        check(unsafe { libc::setgroups(1, one.as_ptr()) })?;
        // SAFETY: plain id changes; failure is reported through errno.
        check(unsafe { libc::setegid(gid) })?;
        // SAFETY: as above.
        check(unsafe { libc::seteuid(uid) })?;
        Ok(guard)
    }

    /// Return to root's effective ids and groups.
    pub fn leave(mut self) -> io::Result<()> {
        self.restore()
    }

    fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        // SAFETY: plain id changes; the saved set-user-ID is still 0.
        check(unsafe { libc::seteuid(0) })?;
        // SAFETY: as above.
        check(unsafe { libc::setegid(0) })?;
        // SAFETY: `groups` holds the groups read in `enter`.
        check(unsafe { libc::setgroups(self.groups.len() as _, self.groups.as_ptr()) })
    }
}

impl Drop for AsService {
    fn drop(&mut self) {
        if let Err(e) = self.restore() {
            // Continuing with the wrong identity is not safe.
            eprintln!("fatal: could not restore root's identity: {e}");
            std::process::abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

    /// A uid no test file has, so only marked objects are untrusted.
    const NOBODY_UID: u32 = 4_000_000_000;

    /// A base directory under `/tmp` (root-owned with the sticky bit on Linux
    /// and macOS, so trusted), by its canonical path.
    fn base() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir_in("/tmp").unwrap();
        let path = fs::canonicalize(dir.path()).unwrap();
        (dir, path)
    }

    fn id(path: &Path) -> FileId {
        let m = fs::symlink_metadata(path).unwrap();
        (m.dev(), m.ino())
    }

    /// Trust where the given directories count as the service account's.
    fn marking(dirs: &[&Path]) -> Trust {
        let mut t = Trust::service(NOBODY_UID, NOBODY_UID, None);
        t.marked = dirs.iter().map(|d| id(d)).collect();
        t
    }

    fn resolver(trust: &Trust) -> Resolver<'_> {
        Resolver {
            trust,
            state_root: None,
            check_final: true,
        }
    }

    fn controlled(r: Resolved) -> PathBuf {
        match r {
            Resolved::Controlled { component, .. } => component,
            other => panic!("expected Controlled, got {other:?}"),
        }
    }

    #[test]
    fn verdict_rules() {
        // SAFETY: zeroed `stat` is valid plain data.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let trust = Trust::service(990, 990, Some(980));
        let set = |st: &mut libc::stat, kind: libc::mode_t, mode: u32, uid: u32, gid: u32| {
            st.st_mode = kind | mode as libc::mode_t;
            st.st_uid = uid;
            st.st_gid = gid;
        };
        set(&mut st, libc::S_IFDIR, 0o755, 0, 0);
        assert_eq!(trust.verdict(&st), None);
        set(&mut st, libc::S_IFDIR, 0o755, 990, 0);
        assert!(trust.verdict(&st).is_some());
        set(&mut st, libc::S_IFDIR, 0o755, 980, 0);
        assert!(trust.verdict(&st).is_some(), "old service uid");
        set(&mut st, libc::S_IFLNK, 0o777, 990, 0);
        assert!(trust.verdict(&st).is_some(), "service-owned link");
        set(&mut st, libc::S_IFDIR, 0o775, 0, 990);
        assert!(
            trust.verdict(&st).is_some(),
            "group-writable by service gid"
        );
        set(&mut st, libc::S_IFDIR, 0o775, 0, 50);
        assert_eq!(trust.verdict(&st), None, "another group");
        // A supplementary group of the service account counts as its own.
        let with_groups = Trust::service(990, 990, None).with_groups([50, 0]);
        assert!(with_groups.verdict(&st).is_some(), "supplementary group");
        set(&mut st, libc::S_IFDIR, 0o775, 0, 0);
        assert_eq!(
            with_groups.verdict(&st),
            None,
            "gid 0 is never the service's"
        );
        set(&mut st, libc::S_IFDIR, 0o777, 0, 0);
        assert!(trust.verdict(&st).is_some(), "world-writable");
        set(&mut st, libc::S_IFDIR, 0o1777, 0, 0);
        assert_eq!(trust.verdict(&st), None, "sticky /tmp");
        set(&mut st, libc::S_IFLNK, 0o777, 0, 0);
        assert_eq!(trust.verdict(&st), None, "link mode is meaningless");
        set(&mut st, libc::S_IFREG, 0o666, 0, 0);
        assert!(trust.verdict(&st).is_some(), "world-writable file");
        // Uid 0 is never the service, even when the state dir was root-owned.
        let fresh = Trust::service(990, 990, Some(0));
        set(&mut st, libc::S_IFDIR, 0o755, 0, 0);
        assert_eq!(fresh.verdict(&st), None);
        let strict = Trust::root_only([990]);
        set(&mut st, libc::S_IFDIR, 0o755, 1000, 0);
        assert!(strict.verdict(&st).is_some());
        // Any other directory writable by a group other than root is refused
        // under the root-only rule, unless it has the sticky bit.
        set(&mut st, libc::S_IFDIR, 0o775, 0, 4);
        st.st_dev = 7;
        st.st_ino = 42;
        assert!(strict.verdict(&st).is_some(), "system group directory");
        set(&mut st, libc::S_IFDIR, 0o2775, 0, 8);
        assert!(strict.verdict(&st).is_some(), "setgid group directory");
        set(&mut st, libc::S_IFDIR, 0o1775, 0, 4);
        assert_eq!(strict.verdict(&st), None, "sticky group directory");
        set(&mut st, libc::S_IFDIR, 0o1775, 0, 990);
        assert!(
            strict.verdict(&st).is_some(),
            "sticky service group directory"
        );
        // Ubuntu's /var/log (root:syslog 0775) and Debian's /var/mail (root:mail
        // 2775) are trusted by device and inode, when the service account is not
        // in those groups.
        let known = Trust::root_only([990]).with_group_dirs(vec![(7, 42)]);
        set(&mut st, libc::S_IFDIR, 0o775, 0, 4);
        assert_eq!(
            known.verdict(&st),
            None,
            "well-known system group directory"
        );
        set(&mut st, libc::S_IFDIR, 0o2775, 0, 8);
        assert_eq!(known.verdict(&st), None, "well-known setgid directory");
        set(&mut st, libc::S_IFDIR, 0o775, 0, 990);
        assert!(
            known.verdict(&st).is_some(),
            "well-known dir of a service group"
        );
        st.st_ino = 43;
        set(&mut st, libc::S_IFDIR, 0o775, 0, 4);
        assert!(known.verdict(&st).is_some(), "another directory");
        // The caller's own files count as trusted for a non-root caller, but a
        // group-writable file never does.
        let caller = Trust {
            also: Some(1000),
            ..Trust::root_only([990])
        };
        set(&mut st, libc::S_IFREG, 0o640, 1000, 1000);
        assert_eq!(caller.verdict(&st), None, "caller's file");
        set(&mut st, libc::S_IFREG, 0o660, 1000, 1000);
        assert!(
            caller.verdict(&st).is_some(),
            "caller's group-writable file"
        );
        set(&mut st, libc::S_IFDIR, 0o775, 0, 990);
        assert!(strict.verdict(&st).is_some(), "service group directory");
        set(&mut st, libc::S_IFREG, 0o660, 0, 4);
        assert!(strict.verdict(&st).is_some(), "group-writable file");
        set(&mut st, libc::S_IFREG, 0o640, 0, 4);
        assert_eq!(strict.verdict(&st), None, "installed config file");
        set(&mut st, libc::S_IFDIR, 0o1777, 0, 0);
        assert_eq!(strict.verdict(&st), None, "sticky /tmp");
    }

    #[test]
    fn account_lookups_tell_a_missing_account_from_a_named_one() {
        assert_eq!(account_name(0).unwrap().as_deref(), Some("root"));
        assert_eq!(account_name(NOBODY_UID).unwrap(), None);
        let root = account_groups_checked("root").unwrap().unwrap();
        assert!(root.contains(&0), "{root:?}");
        assert_eq!(
            account_groups_checked("madmail-test-no-such-account").unwrap(),
            None
        );
        // A lookup error is an error, not "no account".
        let e = passwd_lookup(|_, _, _| libc::EIO, |_| ()).unwrap_err();
        assert_eq!(e.raw_os_error(), Some(libc::EIO));
        assert!(passwd_lookup(|_, _, _| libc::ENOENT, |_| ())
            .unwrap()
            .is_none());
        // Codes POSIX also allows for "not found" can come from an NSS module
        // for an account that exists: they are errors, never "no account".
        for rc in [libc::EPERM, libc::EBADF, libc::ESRCH] {
            let e = passwd_lookup(|_, _, _| rc, |_| ()).unwrap_err();
            assert_eq!(e.raw_os_error(), Some(rc));
        }
    }

    #[test]
    fn getent_output_is_read_by_exit_status() {
        let entry = b"ldapuser:x:4242:4343:LDAP user:/home/ldapuser:/bin/sh\n";
        assert_eq!(
            parse_getent(Some(0), entry, 0, "4242").unwrap().as_deref(),
            Some("ldapuser")
        );
        assert_eq!(
            parse_getent(Some(0), entry, 3, "ldapuser")
                .unwrap()
                .as_deref(),
            Some("4343")
        );
        // Exit status 2: no such entry.
        assert_eq!(parse_getent(Some(2), b"", 0, "99999").unwrap(), None);
        // Anything else fails closed: a usage error, an unsupported database,
        // a signal, or an entry that cannot be read.
        for code in [Some(1), Some(3), Some(127), None] {
            assert!(parse_getent(code, b"", 0, "4242").is_err(), "{code:?}");
        }
        assert!(parse_getent(Some(0), b"garbage\n", 0, "4242").is_err());
        assert!(parse_getent(Some(0), b"", 0, "4242").is_err());
    }

    #[test]
    fn a_directory_owned_by_the_service_uid_is_refused() {
        // The caller's own uid as the service: the caller-owned test directory
        // below the root-owned /tmp is then service-controlled.
        let (_b, b) = base();
        fs::write(b.join("f"), "x").unwrap();
        // SAFETY: getuid has no preconditions.
        let trust = Trust::service(unsafe { libc::getuid() }, 0, None);
        let c = controlled(resolver(&trust).resolve(&b.join("f"), Want::File).unwrap());
        assert_eq!(c, b);
    }

    #[test]
    fn trusted_chain_is_followed() {
        let (_b, b) = base();
        fs::create_dir(b.join("live")).unwrap();
        fs::write(b.join("live/cert.pem"), "x").unwrap();
        symlink(b.join("live"), b.join("hop1")).unwrap();
        symlink("hop1/cert.pem", b.join("hop2")).unwrap();
        let trust = marking(&[]);
        let r = resolver(&trust)
            .resolve(&b.join("hop2"), Want::File)
            .unwrap();
        let Resolved::Found(opened) = r else {
            panic!("{r:?}")
        };
        assert_eq!(opened.path, b.join("live/cert.pem"));
        assert_eq!(kind(&opened.st), libc::S_IFREG);
    }

    #[test]
    fn intermediate_link_into_a_service_dir_is_refused() {
        let (_b, b) = base();
        let svc = b.join("svc");
        fs::create_dir(&svc).unwrap();
        fs::write(svc.join("file"), "x").unwrap();
        symlink(&svc, b.join("op-link")).unwrap();
        let trust = marking(&[&svc]);
        let c = controlled(
            resolver(&trust)
                .resolve(&b.join("op-link/file"), Want::File)
                .unwrap(),
        );
        assert_eq!(c, svc);
    }

    #[test]
    fn second_hop_through_a_service_dir_is_refused_even_if_target_is_outside() {
        // op/cert.pem -> op2/next.pem -> svc/hop -> /etc/hosts (outside, trusted).
        let (_b, b) = base();
        for d in ["op", "op2", "svc"] {
            fs::create_dir(b.join(d)).unwrap();
        }
        let outside = b.join("outside.pem");
        fs::write(&outside, "x").unwrap();
        symlink(&outside, b.join("svc/hop")).unwrap();
        symlink(b.join("svc/hop"), b.join("op2/next.pem")).unwrap();
        symlink(b.join("op2/next.pem"), b.join("op/cert.pem")).unwrap();
        let svc = b.join("svc");
        let trust = marking(&[&svc]);
        let c = controlled(
            resolver(&trust)
                .resolve(&b.join("op/cert.pem"), Want::File)
                .unwrap(),
        );
        assert_eq!(c, svc);
        // Without the mark the same chain reaches the outside file.
        let open = marking(&[]);
        let r = resolver(&open)
            .resolve(&b.join("op/cert.pem"), Want::File)
            .unwrap();
        assert!(
            matches!(r, Resolved::Found(ref o) if o.path == outside),
            "{r:?}"
        );
        // A link owned by the service is refused itself, wherever it points.
        let link_trust = marking(&[&b.join("op2/next.pem")]);
        let c = controlled(
            resolver(&link_trust)
                .resolve(&b.join("op/cert.pem"), Want::File)
                .unwrap(),
        );
        assert_eq!(c, b.join("op2/next.pem"));
    }

    #[test]
    fn hop_limit_and_final_links() {
        let (_b, b) = base();
        symlink(b.join("b"), b.join("a")).unwrap();
        symlink(b.join("a"), b.join("b")).unwrap();
        let trust = marking(&[]);
        let e = resolver(&trust)
            .resolve(&b.join("a"), Want::File)
            .unwrap_err();
        assert_eq!(e.raw_os_error(), Some(libc::ELOOP));
        // A final link to a directory is followed for Want::Dir and is an
        // error for Want::File.
        fs::create_dir(b.join("d")).unwrap();
        symlink(b.join("d"), b.join("dl")).unwrap();
        let r = resolver(&trust).resolve(&b.join("dl"), Want::Dir).unwrap();
        assert!(
            matches!(r, Resolved::Found(ref o) if o.path == b.join("d")),
            "{r:?}"
        );
        let e = resolver(&trust)
            .resolve(&b.join("dl"), Want::File)
            .unwrap_err();
        assert_eq!(e.raw_os_error(), Some(libc::EISDIR));
        // A dangling final link reports its missing target.
        symlink(b.join("nowhere"), b.join("dangling")).unwrap();
        let r = resolver(&trust)
            .resolve(&b.join("dangling"), Want::File)
            .unwrap();
        assert!(
            matches!(r, Resolved::Missing { ref name, .. } if name == "nowhere"),
            "{r:?}"
        );
        // `..` after a link applies to the link's target directory.
        fs::create_dir(b.join("d/sub")).unwrap();
        symlink(b.join("d/sub"), b.join("sl")).unwrap();
        let r = resolver(&trust)
            .resolve(&b.join("sl/../sub"), Want::Dir)
            .unwrap();
        assert!(
            matches!(r, Resolved::Found(ref o) if o.path == b.join("d/sub")),
            "{r:?}"
        );
    }

    #[test]
    fn final_object_is_checked_only_when_asked() {
        let (_b, b) = base();
        let f = b.join("f");
        fs::write(&f, "x").unwrap();
        let trust = marking(&[&f]);
        let r = resolver(&trust).resolve(&f, Want::File).unwrap();
        assert_eq!(controlled(r), f);
        let lenient = Resolver {
            trust: &trust,
            state_root: None,
            check_final: false,
        };
        assert!(matches!(
            lenient.resolve(&f, Want::File).unwrap(),
            Resolved::Found(_)
        ));
    }

    #[test]
    fn state_root_is_recognized_by_inode() {
        let (_b, b) = base();
        let state = b.join("state");
        fs::create_dir_all(state.join("certs")).unwrap();
        // The service replaced its in-tree file with a link out of the tree.
        symlink("/etc/hosts", state.join("certs/fullchain.pem")).unwrap();
        // The operator's link points at the in-tree file.
        symlink(state.join("certs/fullchain.pem"), b.join("op.pem")).unwrap();
        let trust = marking(&[&state]);
        let r = Resolver {
            trust: &trust,
            state_root: Some(id(&state)),
            check_final: false,
        };
        let res = r.resolve(&b.join("op.pem"), Want::File).unwrap();
        assert!(
            matches!(res, Resolved::InTree(ref rest) if rest == Path::new("certs/fullchain.pem")),
            "{res:?}"
        );
        let res = r.resolve(&state, Want::Dir).unwrap();
        assert!(matches!(res, Resolved::InTree(ref rest) if rest.as_os_str().is_empty()));
    }

    #[test]
    fn tree_writes_never_follow_links() {
        let (_b, b) = base();
        let state = b.join("state");
        let out = b.join("out");
        fs::create_dir(&state).unwrap();
        fs::create_dir(&out).unwrap();
        fs::write(out.join("victim"), "keep").unwrap();
        let root = openat(
            libc::AT_FDCWD,
            &cstring(state.as_os_str()).unwrap(),
            DIR_FLAGS,
            0,
        )
        .unwrap();
        // A directory swapped for a link is refused.
        symlink(&out, state.join("dkim")).unwrap();
        let e = tree_open_dir(&root, Path::new("dkim"), true).unwrap_err();
        assert!(
            matches!(e.raw_os_error(), Some(libc::ELOOP) | Some(libc::ENOTDIR)),
            "{e:?}"
        );
        fs::remove_file(state.join("dkim")).unwrap();
        let dkim = tree_open_dir(&root, Path::new("dkim"), true).unwrap();
        // A file link and a hard link are replaced, not written through.
        symlink(out.join("victim"), state.join("dkim/default.txt")).unwrap();
        replace_file(
            dkim.as_raw_fd(),
            OsStr::new("default.txt"),
            b"new",
            0o644,
            None,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(out.join("victim")).unwrap(), "keep");
        assert_eq!(
            fs::read_to_string(state.join("dkim/default.txt")).unwrap(),
            "new"
        );
        fs::hard_link(out.join("victim"), state.join("dkim/hard")).unwrap();
        replace_file(dkim.as_raw_fd(), OsStr::new("hard"), b"new", 0o640, None).unwrap();
        assert_eq!(fs::read_to_string(out.join("victim")).unwrap(), "keep");
        let m = fs::metadata(state.join("dkim/hard")).unwrap();
        assert_eq!((m.permissions().mode() & 0o777, m.nlink()), (0o640, 1));
        // create_new_file refuses any existing entry, including a link.
        symlink(out.join("victim"), state.join("dkim/default.private")).unwrap();
        let created = create_new_file(
            dkim.as_raw_fd(),
            OsStr::new("default.private"),
            b"k",
            0o600,
            None,
        )
        .unwrap();
        assert!(created.is_none());
        // A new file's stat is returned, so callers can record what they made.
        let st = create_new_file(dkim.as_raw_fd(), OsStr::new("fresh"), b"k", 0o600, None)
            .unwrap()
            .unwrap();
        assert_eq!(file_id(&st), id(&state.join("dkim/fresh")));
        let st = replace_file(dkim.as_raw_fd(), OsStr::new("fresh"), b"k2", 0o600, None).unwrap();
        assert_eq!(file_id(&st), id(&state.join("dkim/fresh")));
        assert_eq!(fs::read_to_string(out.join("victim")).unwrap(), "keep");
        // A link, a hard-linked file and a missing file are not read.
        assert!(open_single_link_file(dkim.as_raw_fd(), OsStr::new("hard")).is_ok());
        fs::hard_link(state.join("dkim/hard"), state.join("dkim/hard2")).unwrap();
        assert!(open_single_link_file(dkim.as_raw_fd(), OsStr::new("hard")).is_err());
        assert!(open_single_link_file(dkim.as_raw_fd(), OsStr::new("default.private")).is_err());
        assert!(
            open_single_link_file(dkim.as_raw_fd(), OsStr::new("absent"))
                .unwrap()
                .is_none()
        );
        // `..` cannot leave the tree.
        assert!(tree_open_dir(&root, Path::new("dkim/../.."), false).is_err());
        // No temporary files are left behind.
        let leftovers = fs::read_dir(state.join("dkim"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn remove_dir_tree_works_through_descriptors_and_never_follows_links() {
        let (_b, b) = base();
        let outside = b.join("outside");
        fs::create_dir_all(outside.join("dir")).unwrap();
        fs::write(outside.join("dir/keep"), "x").unwrap();
        fs::write(outside.join("file"), "x").unwrap();
        let tree = b.join("madmail");
        fs::create_dir_all(tree.join("a/b/c")).unwrap();
        fs::write(tree.join("a/b/c/file"), "x").unwrap();
        fs::write(tree.join("top"), "x").unwrap();
        symlink(outside.join("dir"), tree.join("a/dir-link")).unwrap();
        symlink(outside.join("file"), tree.join("a/b/file-link")).unwrap();
        symlink(tree.join("missing"), tree.join("dangling")).unwrap();
        fs::hard_link(outside.join("file"), tree.join("hard")).unwrap();
        let parent = openat(
            libc::AT_FDCWD,
            &cstring(b.as_os_str()).unwrap(),
            DIR_FLAGS,
            0,
        )
        .unwrap();
        let name = cstring(OsStr::new("madmail")).unwrap();
        // A directory other than the one checked is not removed.
        let err = remove_dir_tree(parent.as_raw_fd(), &name, id(&outside)).unwrap_err();
        assert!(err.to_string().contains("replaced"), "{err}");
        assert!(tree.join("top").is_file());
        remove_dir_tree(parent.as_raw_fd(), &name, id(&tree)).unwrap();
        assert!(fs::symlink_metadata(&tree).is_err());
        assert!(outside.join("dir/keep").is_file());
        assert_eq!(fs::read_to_string(outside.join("file")).unwrap(), "x");
        // A link in place of the directory is not followed.
        symlink(&outside, &tree).unwrap();
        let err = remove_dir_tree(parent.as_raw_fd(), &name, id(&outside)).unwrap_err();
        assert!(
            matches!(err.raw_os_error(), Some(libc::ELOOP) | Some(libc::ENOTDIR)),
            "{err}"
        );
        assert!(outside.join("dir/keep").is_file());
    }

    #[test]
    fn ensure_dir_creates_missing_components_only_in_trusted_dirs() {
        let (_b, b) = base();
        let trust = marking(&[]);
        let target = b.join("a/b/c");
        let r = resolver(&trust).ensure_dir(&target).unwrap();
        assert!(matches!(r, Resolved::Found(_)), "{r:?}");
        assert!(target.is_dir());
        let svc = b.join("svc");
        fs::create_dir(&svc).unwrap();
        let marked = marking(&[&svc]);
        let r = resolver(&marked).ensure_dir(&svc.join("x/y")).unwrap();
        assert_eq!(controlled(r), svc);
        assert!(!svc.join("x").exists());
    }
}
