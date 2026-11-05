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

//! Recursive ownership change of the service-writable state tree, by file
//! descriptor.
//!
//! Root runs this walk while the service account may be changing the same tree:
//! it can create, rename (including `renameat2(RENAME_EXCHANGE)` loops) and link
//! anything there. A walk that uses path names checks one object and then changes
//! another one, for example a directory swapped for a link to `/etc`. This walk
//! therefore never resolves a path below the root, and it checks and changes each
//! object through one descriptor for that object:
//!
//! - the root is opened with `O_NOFOLLOW | O_DIRECTORY` (a root that is a link
//!   is refused; the caller resolves an operator's link with the checked
//!   resolver and passes the descriptor to [`chown_tree_fd`]). Its owner before
//!   the walk is recorded as `old_uid`: the previous service uid when the
//!   account was recreated with a new uid;
//! - each directory is opened relative to its parent's descriptor with
//!   `O_NOFOLLOW | O_DIRECTORY`, so a link swapped in fails to open (`ELOOP` or
//!   `ENOTDIR`) instead of being followed, and is changed with `fchown` on the
//!   opened descriptor (directories cannot be hard-linked);
//! - every other entry is opened by itself and checked with `fstat` on that
//!   descriptor before `fchown`: on Linux `openat(O_PATH | O_NOFOLLOW)` and
//!   `fchownat(fd, "", AT_EMPTY_PATH)`; elsewhere regular files are opened with
//!   `O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_NOCTTY` and changed with `fchown`,
//!   and FIFOs, sockets and device files are left unchanged (they are not
//!   opened);
//! - [`decide`] skips a link, and skips a file with more than one hard link
//!   whose owner is neither the service uid nor `old_uid`: that can only be a
//!   link to a file of another account (for example `/etc/shadow` when
//!   `fs.protected_hardlinks` is 0). The mail store (`chatmail-storage`
//!   `cas.rs` / `blob.rs`) hard-links every message into the maildirs; those
//!   files belong to the service (`uid` or `old_uid`), so they are changed. A
//!   file with one link exists only in the tree and is changed whatever its
//!   owner. Uid 0 is never taken as `old_uid`.
//!
//! An entry that changes or disappears during the walk is skipped with a
//! warning; only problems with the root itself are hard failures. The walk is
//! iterative and stops descending at [`MAX_DEPTH`] levels (each open level holds
//! one descriptor), so a deliberately deep tree cannot exhaust the stack or the
//! descriptor limit.

use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[cfg(test)]
use crate::ctl::secure_fs::cstring;
use crate::ctl::secure_fs::{fchown, fstat, fstatat_nofollow, kind, openat, DIR_FLAGS};

/// Directory levels below the root that are still changed. Install-created
/// state (databases, DKIM keys, mail directories) is a few levels deep.
pub const MAX_DEPTH: usize = 64;

/// A failure that stops the walk.
#[derive(Debug)]
pub struct WalkError {
    pub op: &'static str,
    pub path: PathBuf,
    pub source: io::Error,
}

/// An entry the walk left unchanged.
#[derive(Debug)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: String,
}

/// What the walk does with one object, from its `fstat` data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Chown,
    /// A symbolic link: never followed or changed.
    SkipLink,
    /// A file with several hard links owned by another account.
    SkipForeignHardLink {
        owner: u32,
        links: u64,
    },
}

/// The ownership decision for an object below the root. `uid` is the service
/// uid being applied and `old_uid` the root's owner before the walk; 0 never
/// counts as a service uid.
#[allow(clippy::unnecessary_cast)]
pub fn decide(st: &libc::stat, old_uid: u32, uid: u32) -> Action {
    match kind(st) {
        libc::S_IFLNK => Action::SkipLink,
        libc::S_IFDIR => Action::Chown,
        _ => {
            let service_owned = st.st_uid == uid || (old_uid != 0 && st.st_uid == old_uid);
            if st.st_nlink > 1 && !service_owned {
                Action::SkipForeignHardLink {
                    owner: st.st_uid,
                    links: st.st_nlink as u64,
                }
            } else {
                Action::Chown
            }
        }
    }
}

struct Frame {
    fd: OwnedFd,
    path: PathBuf,
    names: std::vec::IntoIter<CString>,
    depth: usize,
}

/// Change the owner of `root` and everything below it to `uid:gid` without
/// following symbolic links. `root` is opened as given with `O_NOFOLLOW`, so a
/// root that is a link fails with `ELOOP`. Install opens the root with the
/// checked resolver instead and calls [`chown_tree_fd`].
#[cfg(test)]
pub fn chown_tree(root: &Path, uid: u32, gid: u32) -> Result<Vec<Skipped>, WalkError> {
    let fail = |source| WalkError {
        op: "open",
        path: root.to_path_buf(),
        source,
    };
    let c_root = cstring(root.as_os_str()).map_err(fail)?;
    let fd = openat(libc::AT_FDCWD, &c_root, DIR_FLAGS, 0).map_err(fail)?;
    chown_tree_fd(fd, root, uid, gid)
}

/// [`chown_tree`] for a root directory that is already open (with
/// [`DIR_FLAGS`]); `root_path` is only used in messages.
pub fn chown_tree_fd(
    fd: OwnedFd,
    root_path: &Path,
    uid: u32,
    gid: u32,
) -> Result<Vec<Skipped>, WalkError> {
    let fail = |op, path: &Path, source| WalkError {
        op,
        path: path.to_path_buf(),
        source,
    };
    let root_st = fstat(fd.as_raw_fd()).map_err(|e| fail("stat", root_path, e))?;
    if kind(&root_st) != libc::S_IFDIR {
        return Err(fail(
            "open",
            root_path,
            io::Error::from_raw_os_error(libc::ENOTDIR),
        ));
    }
    let old_uid = root_st.st_uid;
    fchown(fd.as_raw_fd(), uid, gid).map_err(|e| fail("chown", root_path, e))?;
    let names = read_names(&fd).map_err(|e| fail("read_dir", root_path, e))?;

    let mut skipped = Vec::new();
    let mut stack = vec![Frame {
        fd,
        path: root_path.to_path_buf(),
        names: names.into_iter(),
        depth: 0,
    }];
    while let Some(frame) = stack.last_mut() {
        let Some(name) = frame.names.next() else {
            stack.pop();
            continue;
        };
        let path = frame.path.join(OsStr::from_bytes(name.to_bytes()));
        let mut skip = |reason: String| {
            skipped.push(Skipped {
                path: path.clone(),
                reason,
            })
        };
        let st = match fstatat_nofollow(frame.fd.as_raw_fd(), &name) {
            Ok(st) => st,
            Err(e) if changed_under_us(&e) => {
                skip(format!("changed during the walk ({e})"));
                continue;
            }
            Err(e) => return Err(fail("stat", &path, e)),
        };
        match kind(&st) {
            // Links planted by the service account are never followed or changed.
            libc::S_IFLNK => {}
            libc::S_IFDIR => {
                if frame.depth + 1 > MAX_DEPTH {
                    skip(format!("deeper than {MAX_DEPTH} levels"));
                    continue;
                }
                let child = match openat(frame.fd.as_raw_fd(), &name, DIR_FLAGS, 0) {
                    Ok(fd) => fd,
                    Err(e) if changed_under_us(&e) => {
                        skip(format!("changed during the walk ({e})"));
                        continue;
                    }
                    Err(e) => return Err(fail("open", &path, e)),
                };
                fchown(child.as_raw_fd(), uid, gid).map_err(|e| fail("chown", &path, e))?;
                let names = match read_names(&child) {
                    Ok(names) => names,
                    Err(e) if changed_under_us(&e) => {
                        skip(format!("changed during the walk ({e})"));
                        continue;
                    }
                    Err(e) => return Err(fail("read_dir", &path, e)),
                };
                let depth = frame.depth + 1;
                stack.push(Frame {
                    fd: child,
                    path,
                    names: names.into_iter(),
                    depth,
                });
            }
            entry_kind => {
                match chown_entry(frame.fd.as_raw_fd(), &name, entry_kind, old_uid, uid, gid) {
                    Ok(None) => {}
                    Ok(Some(reason)) => skip(reason),
                    Err(e) if changed_under_us(&e) => {
                        skip(format!("changed during the walk ({e})"))
                    }
                    Err(e) => return Err(fail("chown", &path, e)),
                }
            }
        }
    }
    Ok(skipped)
}

fn skip_reason(action: Action) -> Option<String> {
    match action {
        Action::Chown | Action::SkipLink => None,
        Action::SkipForeignHardLink { owner, links } => Some(format!(
            "has {links} hard links and belongs to uid {owner}, not to the service account; \
             it may be a link to a file outside the state directory"
        )),
    }
}

/// Check and change one non-directory entry through a descriptor for it.
/// Returns why it was left unchanged, if it was.
#[cfg(target_os = "linux")]
fn chown_entry(
    dir: RawFd,
    name: &CStr,
    _kind: libc::mode_t,
    old_uid: u32,
    uid: u32,
    gid: u32,
) -> io::Result<Option<String>> {
    // O_PATH opens the entry itself (a link too) without reading it.
    let fd = openat(
        dir,
        name,
        libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        0,
    )?;
    let st = fstat(fd.as_raw_fd())?;
    if kind(&st) == libc::S_IFDIR {
        return Err(io::Error::from_raw_os_error(libc::ENOTDIR));
    }
    let action = decide(&st, old_uid, uid);
    if action == Action::Chown {
        // SAFETY: `fd` is open; the empty path with AT_EMPTY_PATH names the
        // object `fd` refers to.
        let rc =
            unsafe { libc::fchownat(fd.as_raw_fd(), c"".as_ptr(), uid, gid, libc::AT_EMPTY_PATH) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(skip_reason(action))
}

#[cfg(not(target_os = "linux"))]
fn chown_entry(
    dir: RawFd,
    name: &CStr,
    entry_kind: libc::mode_t,
    old_uid: u32,
    uid: u32,
    gid: u32,
) -> io::Result<Option<String>> {
    if entry_kind != libc::S_IFREG {
        // Without O_PATH, a FIFO, socket or device would have to be opened.
        return Ok(Some(
            "not a regular file, directory or link; left unchanged on this platform".into(),
        ));
    }
    let fd = openat(dir, name, crate::ctl::secure_fs::FILE_FLAGS, 0)?;
    let st = fstat(fd.as_raw_fd())?;
    if kind(&st) != libc::S_IFREG {
        return Err(io::Error::from_raw_os_error(libc::ENOENT));
    }
    let action = decide(&st, old_uid, uid);
    if action == Action::Chown {
        fchown(fd.as_raw_fd(), uid, gid)?;
    }
    Ok(skip_reason(action))
}

/// Errors that mean an entry was replaced or removed after it was listed.
fn changed_under_us(e: &io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(libc::ELOOP) | Some(libc::ENOTDIR) | Some(libc::ENOENT)
    )
}

/// List the entries of the open directory `fd`, except `.` and `..`. The
/// listing uses a duplicate descriptor, so `fd` stays open for `*at` calls.
fn read_names(fd: &OwnedFd) -> io::Result<Vec<CString>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, MetadataExt};

    fn owner(path: &Path) -> (u32, u32) {
        let m = fs::symlink_metadata(path).unwrap();
        (m.uid(), m.gid())
    }

    fn stat_of(kind: libc::mode_t, nlink: u32, uid: u32) -> libc::stat {
        // SAFETY: zeroed `stat` is valid plain data.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        st.st_mode = kind | 0o644;
        st.st_nlink = nlink as _;
        st.st_uid = uid;
        st
    }

    #[test]
    fn decide_skips_links_and_foreign_hard_links_only() {
        let (old, new) = (980, 990);
        // Hard-linked message files of the old or the new service uid are
        // changed: the repair case after the account got a new uid.
        assert_eq!(
            decide(&stat_of(libc::S_IFREG, 2, old), old, new),
            Action::Chown
        );
        assert_eq!(
            decide(&stat_of(libc::S_IFREG, 3, new), old, new),
            Action::Chown
        );
        // A hard link to a file of another account (root, another user).
        for foreign in [0, 1000] {
            assert_eq!(
                decide(&stat_of(libc::S_IFREG, 2, foreign), old, new),
                Action::SkipForeignHardLink {
                    owner: foreign,
                    links: 2
                }
            );
        }
        // Uid 0 is never the old service uid, even when the root was root-owned.
        assert!(matches!(
            decide(&stat_of(libc::S_IFREG, 2, 0), 0, new),
            Action::SkipForeignHardLink { .. }
        ));
        // A single-link file exists only in the tree: changed whatever its owner.
        assert_eq!(
            decide(&stat_of(libc::S_IFREG, 1, 0), old, new),
            Action::Chown
        );
        assert_eq!(
            decide(&stat_of(libc::S_IFIFO, 1, 1000), old, new),
            Action::Chown
        );
        // Directories have nlink >= 2 by nature and cannot be hard-linked.
        assert_eq!(
            decide(&stat_of(libc::S_IFDIR, 7, 0), old, new),
            Action::Chown
        );
        assert_eq!(
            decide(&stat_of(libc::S_IFLNK, 1, new), old, new),
            Action::SkipLink
        );
    }

    #[test]
    fn walks_nested_tree_and_skips_links() {
        let state = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = state.path();
        fs::create_dir_all(root.join("a/b/c")).unwrap();
        fs::write(root.join("a/b/c/file"), "x").unwrap();
        fs::write(outside.path().join("target"), "x").unwrap();
        // A link nested below the root, to a directory and to a file outside.
        symlink(outside.path(), root.join("a/b/dir-link")).unwrap();
        symlink(outside.path().join("target"), root.join("a/file-link")).unwrap();

        let (uid, gid) = owner(root);
        let skipped = chown_tree(root, uid, gid).unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(owner(&root.join("a/b/c/file")), (uid, gid));
    }

    #[test]
    fn service_hard_links_are_changed_and_foreign_ones_skipped() {
        let state = tempfile::tempdir().unwrap();
        let root = state.path();
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("mail/new")).unwrap();
        fs::write(root.join("blobs/msg"), "x").unwrap();
        fs::hard_link(root.join("blobs/msg"), root.join("mail/new/msg")).unwrap();
        assert_eq!(
            fs::symlink_metadata(root.join("blobs/msg"))
                .unwrap()
                .nlink(),
            2
        );
        // SAFETY: geteuid has no preconditions.
        let as_root = unsafe { libc::geteuid() } == 0;
        // The caller owns the root and the files: they are the old service uid.
        // As root, give them to an old service uid first, then hand them to a
        // new one, as after the service account was recreated with another uid.
        let (uid, gid) = if as_root {
            for p in ["", "blobs", "blobs/msg", "mail", "mail/new", "mail/new/msg"] {
                std::os::unix::fs::chown(root.join(p), Some(4242), Some(4242)).unwrap();
            }
            (4243, 4243)
        } else {
            owner(root)
        };
        let skipped = chown_tree(root, uid, gid).unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(owner(&root.join("mail/new/msg")), (uid, gid));
        assert_eq!(owner(&root.join("blobs/msg")), (uid, gid));
        // As a non-root caller the only foreign file available is a root-owned
        // one, which cannot be hard-linked without privilege on most systems;
        // the Docker suite covers it and `decide` covers the rule itself.
        if as_root {
            let outside = tempfile::tempdir().unwrap();
            let victim = outside.path().join("victim");
            fs::write(&victim, "x").unwrap();
            fs::hard_link(&victim, root.join("planted")).unwrap();
            let skipped = chown_tree(root, 4244, 4244).unwrap();
            assert_eq!(skipped.len(), 1, "{skipped:?}");
            assert_eq!(owner(&victim), (0, 0));
        }
    }

    #[test]
    fn refuses_a_root_that_is_a_link() {
        let state = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let link = outside.path().join("state-link");
        symlink(state.path(), &link).unwrap();
        let (uid, gid) = owner(state.path());
        let err = chown_tree(&link, uid, gid).unwrap_err();
        assert_eq!(err.op, "open");
        assert_eq!(err.path, link);
        assert!(changed_under_us(&err.source), "{err:?}");
    }

    #[test]
    fn stops_descending_at_the_depth_limit() {
        let state = tempfile::tempdir().unwrap();
        let mut deep = state.path().to_path_buf();
        for _ in 0..MAX_DEPTH + 3 {
            deep.push("d");
        }
        fs::create_dir_all(&deep).unwrap();
        let (uid, gid) = owner(state.path());
        let skipped = chown_tree(state.path(), uid, gid).unwrap();
        assert_eq!(skipped.len(), 1, "{skipped:?}");
        assert_eq!(
            skipped[0]
                .path
                .strip_prefix(state.path())
                .unwrap()
                .iter()
                .count(),
            MAX_DEPTH + 1
        );
    }

    #[test]
    fn missing_entries_are_reported_not_fatal() {
        let state = tempfile::tempdir().unwrap();
        let c_root = cstring(state.path().as_os_str()).unwrap();
        let fd = openat(libc::AT_FDCWD, &c_root, DIR_FLAGS, 0).unwrap();
        let name = CString::new("vanished").unwrap();
        let e = fstatat_nofollow(fd.as_raw_fd(), &name).unwrap_err();
        assert!(changed_under_us(&e));
        let e = chown_entry(fd.as_raw_fd(), &name, libc::S_IFREG, 0, 0, 0).unwrap_err();
        assert!(changed_under_us(&e), "{e:?}");
        fs::write(state.path().join("file"), "x").unwrap();
        let file = CString::new("file").unwrap();
        let e = openat(fd.as_raw_fd(), &file, DIR_FLAGS, 0).unwrap_err();
        assert!(changed_under_us(&e), "{e:?}");
        symlink(state.path(), state.path().join("link")).unwrap();
        let link = CString::new("link").unwrap();
        let e = openat(fd.as_raw_fd(), &link, DIR_FLAGS, 0).unwrap_err();
        assert!(changed_under_us(&e), "{e:?}");
    }

    #[test]
    fn lists_names_without_dot_entries() {
        let state = tempfile::tempdir().unwrap();
        fs::write(state.path().join("one"), "x").unwrap();
        fs::create_dir(state.path().join("two")).unwrap();
        let c_root = cstring(state.path().as_os_str()).unwrap();
        let fd = openat(libc::AT_FDCWD, &c_root, DIR_FLAGS, 0).unwrap();
        let mut names: Vec<_> = read_names(&fd)
            .unwrap()
            .into_iter()
            .map(|n| n.into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["one", "two"]);
        // The original descriptor is still usable after listing.
        assert!(fstatat_nofollow(fd.as_raw_fd(), &CString::new("one").unwrap()).is_ok());
    }
}
