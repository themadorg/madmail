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
//!   resolver and passes the descriptor to [`chown_tree_fd`]). The caller
//!   records the root's owner before the walk as `old_uid`: the previous
//!   service uid when the account was recreated with a new uid (install leaves
//!   it out when that uid now belongs to another account). The root itself is
//!   changed last, so a run that stops half way can be repeated with the same
//!   `old_uid`;
//! - each directory is opened relative to its parent's descriptor with
//!   `O_NOFOLLOW | O_DIRECTORY`, so a link swapped in fails to open (`ELOOP` or
//!   `ENOTDIR`) instead of being followed, then checked with `fstat` and
//!   changed with `fchown` on the opened descriptor (directories cannot be
//!   hard-linked);
//! - every other entry is opened by itself and checked with `fstat` on that
//!   descriptor before `fchown`: on Linux `openat(O_PATH | O_NOFOLLOW)` and
//!   `fchownat(fd, "", AT_EMPTY_PATH)`; elsewhere regular files are opened with
//!   `O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_NOCTTY` and changed with `fchown`,
//!   and FIFOs, sockets and device files are left unchanged (they are not
//!   opened);
//! - [`decide`] skips a link, a file whose owner is neither the service uid nor
//!   `old_uid` (whatever its link count), and a directory whose owner is none
//!   of the service uid, `old_uid` and root (with everything below it). The
//!   link count proves nothing: a hard link to another account's file (for
//!   example `/etc/shadow`, possible when `fs.protected_hardlinks` is 0)
//!   becomes the inode's only link once the original name is replaced by a
//!   rename. The mail store (`chatmail-storage` `cas.rs` / `blob.rs`)
//!   hard-links every message into the maildirs; those files belong to the
//!   service (`uid` or `old_uid`), so they are changed. Uid 0 is never taken
//!   as `old_uid`. Install creates the files it writes into the tree with the
//!   service owner already (see `SecureInstall`), so the walk needs no list of
//!   them.
//!
//! An entry that changes or disappears during the walk is skipped with a
//! warning; only problems with the root itself are hard failures. The walk is
//! iterative and stops descending at [`MAX_DEPTH`] levels (each open level holds
//! one descriptor), so a deliberately deep tree cannot exhaust the stack or the
//! descriptor limit.

use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::system::STATE_MARKER;

#[cfg(test)]
use crate::ctl::secure_fs::cstring;
#[cfg(not(target_os = "linux"))]
use crate::ctl::secure_fs::fchown;
use crate::ctl::secure_fs::{fstat, fstatat_nofollow, kind, openat, read_names, DIR_FLAGS};

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
    /// The entry belongs to another account ([`Action::SkipForeignOwner`]).
    pub foreign_owner: bool,
    /// The entry's owner, for an entry of another account.
    pub owner: Option<u32>,
    /// The entry is a directory, so it was not entered either.
    pub is_dir: bool,
}

/// How the walk gives one checked object to `uid:gid`, through the descriptor
/// it checked: a directory opened with [`DIR_FLAGS`], or another entry (on
/// Linux opened with `O_PATH`). [`ByDescriptor`] is the real one; tests record
/// the calls and inject errors.
pub trait Chown {
    fn chown(&mut self, fd: RawFd, path: &Path, uid: u32, gid: u32) -> io::Result<()>;
}

/// `fchown`, or on Linux `fchownat(fd, "", AT_EMPTY_PATH)`, which also works
/// on an `O_PATH` descriptor.
pub struct ByDescriptor;

impl Chown for ByDescriptor {
    fn chown(&mut self, fd: RawFd, _path: &Path, uid: u32, gid: u32) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            // SAFETY: `fd` is open; the empty path with AT_EMPTY_PATH names
            // the object `fd` refers to.
            let rc = unsafe { libc::fchownat(fd, c"".as_ptr(), uid, gid, libc::AT_EMPTY_PATH) };
            if rc < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        fchown(fd, uid, gid)
    }
}

/// What the walk does with one object, from its `fstat` data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Chown,
    /// A symbolic link: never followed or changed.
    SkipLink,
    /// An object owned by another account: a file not owned by the service
    /// uid or `old_uid`, or a directory not owned by those or by root.
    SkipForeignOwner {
        owner: u32,
        links: u64,
    },
    /// A FIFO, socket or device file where it could only be changed after
    /// opening it (not Linux).
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    SkipSpecial,
}

/// The ownership decision for an object below the root. `uid` is the service
/// uid being applied and `old_uid` the previous service uid (never 0).
///
/// Files are changed only when they belong to `uid` or `old_uid`. Directories
/// are also changed when root owns them (install and root commands create
/// them; a directory cannot be hard-linked, and one root owns cannot be moved
/// into the tree by another account, which would need write access to it).
#[allow(clippy::unnecessary_cast)]
pub fn decide(st: &libc::stat, old_uid: Option<u32>, uid: u32) -> Action {
    let service_owned = st.st_uid == uid || old_uid.is_some_and(|old| old != 0 && st.st_uid == old);
    let foreign = Action::SkipForeignOwner {
        owner: st.st_uid,
        links: st.st_nlink as u64,
    };
    match kind(st) {
        libc::S_IFLNK => Action::SkipLink,
        libc::S_IFDIR if service_owned || st.st_uid == 0 => Action::Chown,
        _ if service_owned => Action::Chown,
        _ => foreign,
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
/// root that is a link fails with `ELOOP`; the root's current owner is taken
/// as `old_uid`. Install opens the root with the checked resolver instead and
/// calls [`chown_tree_fd`].
#[cfg(test)]
pub fn chown_tree(root: &Path, uid: u32, gid: u32) -> Result<Vec<Skipped>, WalkError> {
    let fail = |source| WalkError {
        op: "open",
        path: root.to_path_buf(),
        source,
    };
    let c_root = cstring(root.as_os_str()).map_err(fail)?;
    let fd = openat(libc::AT_FDCWD, &c_root, DIR_FLAGS, 0).map_err(fail)?;
    let old_uid = fstat(fd.as_raw_fd()).map_err(fail)?.st_uid;
    chown_tree_fd(fd, root, uid, gid, Some(old_uid))
}

/// [`chown_tree`] for a root directory that is already open (with
/// [`DIR_FLAGS`]); `root_path` is only used in messages. `old_uid` is the
/// previous service uid, if any (see [`decide`]). The root is always changed,
/// after everything below it, and not at all when the walk fails. madmail's
/// [`STATE_MARKER`] directly in the root, when it is a regular file with one
/// link that belongs to root, is left as it is without a note: it stays
/// root's, so that it only ever counts as made by root (see
/// `check_state_root`).
pub fn chown_tree_fd(
    fd: OwnedFd,
    root_path: &Path,
    uid: u32,
    gid: u32,
    old_uid: Option<u32>,
) -> Result<Vec<Skipped>, WalkError> {
    chown_tree_fd_with(fd, root_path, uid, gid, old_uid, &mut ByDescriptor)
}

/// [`chown_tree_fd`] with the ownership change given by `owner`.
pub fn chown_tree_fd_with(
    fd: OwnedFd,
    root_path: &Path,
    uid: u32,
    gid: u32,
    old_uid: Option<u32>,
    owner: &mut dyn Chown,
) -> Result<Vec<Skipped>, WalkError> {
    let marker = CString::new(STATE_MARKER).expect("no NUL in the marker name");
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
    let rules = Rules {
        old_uid: old_uid.filter(|&u| u != 0),
        uid,
        gid,
    };
    let root_fd = fd.try_clone().map_err(|e| fail("open", root_path, e))?;
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
        let mut skip = |action: Action, is_dir: bool| {
            if let Some(reason) = skip_reason(action) {
                skipped.push(Skipped {
                    path: path.clone(),
                    reason,
                    foreign_owner: matches!(action, Action::SkipForeignOwner { .. }),
                    owner: match action {
                        Action::SkipForeignOwner { owner, .. } => Some(owner),
                        _ => None,
                    },
                    is_dir,
                })
            }
        };
        let st = match fstatat_nofollow(frame.fd.as_raw_fd(), &name) {
            Ok(st) => st,
            Err(e) if changed_under_us(&e) => {
                push_changed(&mut skipped, &path, &e);
                continue;
            }
            Err(e) => return Err(fail("stat", &path, e)),
        };
        match kind(&st) {
            // Links planted by the service account are never followed or changed.
            libc::S_IFLNK => {}
            libc::S_IFDIR => {
                if frame.depth + 1 > MAX_DEPTH {
                    skipped.push(Skipped {
                        path: path.clone(),
                        reason: format!("deeper than {MAX_DEPTH} levels"),
                        foreign_owner: false,
                        owner: None,
                        is_dir: true,
                    });
                    continue;
                }
                let opened = openat(frame.fd.as_raw_fd(), &name, DIR_FLAGS, 0)
                    .and_then(|child| fstat(child.as_raw_fd()).map(|st| (child, st)));
                let (child, child_st) = match opened {
                    Ok(opened) => opened,
                    Err(e) if changed_under_us(&e) => {
                        push_changed(&mut skipped, &path, &e);
                        continue;
                    }
                    Err(e) => return Err(fail("open", &path, e)),
                };
                let action = rules.decide(&child_st);
                if action != Action::Chown {
                    // Not descended: another account's directory.
                    skip(action, true);
                    continue;
                }
                owner
                    .chown(child.as_raw_fd(), &path, rules.uid, rules.gid)
                    .map_err(|e| fail("chown", &path, e))?;
                let names = match read_names(&child) {
                    Ok(names) => names,
                    Err(e) if changed_under_us(&e) => {
                        push_changed(&mut skipped, &path, &e);
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
            libc::S_IFREG
                if frame.depth == 0 && name == marker && st.st_uid == 0 && st.st_nlink == 1 => {}
            entry_kind => match chown_entry(
                frame.fd.as_raw_fd(),
                &name,
                &path,
                entry_kind,
                &rules,
                owner,
            ) {
                Ok(action) => skip(action, false),
                Err(e) if changed_under_us(&e) => push_changed(&mut skipped, &path, &e),
                Err(e) => return Err(fail("chown", &path, e)),
            },
        }
    }
    owner
        .chown(root_fd.as_raw_fd(), root_path, uid, gid)
        .map_err(|e| fail("chown", root_path, e))?;
    Ok(skipped)
}

fn changed_reason(e: &io::Error) -> String {
    format!("changed during the walk ({e})")
}

fn push_changed(skipped: &mut Vec<Skipped>, path: &Path, e: &io::Error) {
    skipped.push(Skipped {
        path: path.to_path_buf(),
        reason: changed_reason(e),
        foreign_owner: false,
        owner: None,
        is_dir: false,
    });
}

/// The ids the walk applies to every entry.
struct Rules {
    old_uid: Option<u32>,
    uid: u32,
    gid: u32,
}

impl Rules {
    fn decide(&self, st: &libc::stat) -> Action {
        decide(st, self.old_uid, self.uid)
    }
}

/// Why an entry with this action is left unchanged, for the operator.
pub fn skip_reason(action: Action) -> Option<String> {
    match action {
        Action::Chown | Action::SkipLink => None,
        Action::SkipSpecial => {
            Some("not a regular file, directory or link; left unchanged on this platform".into())
        }
        Action::SkipForeignOwner { owner, links } => {
            let links = if links > 1 {
                format!(" and has {links} hard links")
            } else {
                String::new()
            };
            Some(format!(
                "belongs to uid {owner}, not to the service account{links}; it may be another \
                 account's file"
            ))
        }
    }
}

/// Check and change one non-directory entry through a descriptor for it.
/// Returns the action taken ([`Action::Chown`] when it was changed).
#[cfg(target_os = "linux")]
fn chown_entry(
    dir: RawFd,
    name: &CStr,
    path: &Path,
    _kind: libc::mode_t,
    rules: &Rules,
    owner: &mut dyn Chown,
) -> io::Result<Action> {
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
    let action = rules.decide(&st);
    if action == Action::Chown {
        owner.chown(fd.as_raw_fd(), path, rules.uid, rules.gid)?;
    }
    Ok(action)
}

#[cfg(not(target_os = "linux"))]
fn chown_entry(
    dir: RawFd,
    name: &CStr,
    path: &Path,
    entry_kind: libc::mode_t,
    rules: &Rules,
    owner: &mut dyn Chown,
) -> io::Result<Action> {
    if entry_kind != libc::S_IFREG {
        // Without O_PATH, a FIFO, socket or device would have to be opened.
        return Ok(Action::SkipSpecial);
    }
    let fd = openat(dir, name, crate::ctl::secure_fs::FILE_FLAGS, 0)?;
    let st = fstat(fd.as_raw_fd())?;
    if kind(&st) != libc::S_IFREG {
        return Err(io::Error::from_raw_os_error(libc::ENOENT));
    }
    let action = rules.decide(&st);
    if action == Action::Chown {
        owner.chown(fd.as_raw_fd(), path, rules.uid, rules.gid)?;
    }
    Ok(action)
}

/// Errors that mean an entry was replaced or removed after it was listed.
fn changed_under_us(e: &io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(libc::ELOOP) | Some(libc::ENOTDIR) | Some(libc::ENOENT)
    )
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
    fn decide_changes_only_objects_of_the_service() {
        let (old, new) = (980, 990);
        let d = |kind, nlink, owner| decide(&stat_of(kind, nlink, owner), Some(old), new);
        // Hard-linked message files of the old or the new service uid are
        // changed: the repair case after the account got a new uid.
        assert_eq!(d(libc::S_IFREG, 2, old), Action::Chown);
        assert_eq!(d(libc::S_IFREG, 3, new), Action::Chown);
        assert_eq!(d(libc::S_IFREG, 1, old), Action::Chown);
        // A file of another account (root, another user) is skipped whatever
        // its link count: a hard link to /etc/shadow becomes the only link once
        // /etc/shadow is replaced by a rename, and a hard link to the config
        // file root just wrote is root's too.
        for foreign in [0, 1000] {
            for links in [1, 2] {
                assert_eq!(
                    d(libc::S_IFREG, links, foreign),
                    Action::SkipForeignOwner {
                        owner: foreign,
                        links: links as u64
                    }
                );
            }
        }
        assert!(matches!(
            d(libc::S_IFIFO, 1, 1000),
            Action::SkipForeignOwner { .. }
        ));
        // Uid 0 is never the old service uid; without one, only `new` counts.
        assert!(matches!(
            decide(&stat_of(libc::S_IFREG, 2, 0), Some(0), new),
            Action::SkipForeignOwner { .. }
        ));
        assert!(matches!(
            decide(&stat_of(libc::S_IFREG, 2, old), None, new),
            Action::SkipForeignOwner { .. }
        ));
        // Directories (nlink >= 2 by nature, never hard-linked) are changed
        // when the service or root owns them, not when another account does.
        assert_eq!(d(libc::S_IFDIR, 7, 0), Action::Chown);
        assert_eq!(d(libc::S_IFDIR, 2, old), Action::Chown);
        assert_eq!(
            d(libc::S_IFDIR, 2, 1000),
            Action::SkipForeignOwner {
                owner: 1000,
                links: 2
            }
        );
        assert_eq!(d(libc::S_IFLNK, 1, new), Action::SkipLink);
        let reason = skip_reason(Action::SkipForeignOwner { owner: 0, links: 2 }).unwrap();
        assert!(
            reason.starts_with("belongs to uid 0, not to the service account and has 2 hard links"),
            "{reason}"
        );
    }

    #[test]
    fn foreign_files_and_dirs_are_skipped_through_their_descriptor() {
        // The caller's own uid is "foreign" here: the walk applies another
        // service uid, with no old uid. Nothing is chowned, so no privilege is
        // needed (a Chown decision would fail with EPERM).
        let state = tempfile::tempdir().unwrap();
        let root = state.path();
        fs::write(root.join("single"), "x").unwrap();
        fs::write(root.join("linked"), "x").unwrap();
        fs::hard_link(root.join("linked"), root.join("linked2")).unwrap();
        let c_root = cstring(root.as_os_str()).unwrap();
        let fd = openat(libc::AT_FDCWD, &c_root, DIR_FLAGS, 0).unwrap();
        let (uid, gid) = owner(root);
        // SAFETY: geteuid has no preconditions.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let rules = Rules {
            old_uid: None,
            uid: uid.wrapping_add(1),
            gid,
        };
        for (name, links) in [("single", 1), ("linked", 2), ("linked2", 2)] {
            let c = CString::new(name).unwrap();
            let action = chown_entry(
                fd.as_raw_fd(),
                &c,
                Path::new(name),
                libc::S_IFREG,
                &rules,
                &mut ByDescriptor,
            )
            .unwrap();
            assert_eq!(action, Action::SkipForeignOwner { owner: uid, links });
        }
        assert_eq!(owner(&root.join("single")), (uid, gid));
        // The whole walk: the root is changed (EPERM here), but only after
        // every entry was looked at and none of them was changed.
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/inner"), "x").unwrap();
        let fd = openat(libc::AT_FDCWD, &c_root, DIR_FLAGS, 0).unwrap();
        let err = chown_tree_fd(fd, root, uid.wrapping_add(1), gid, None).unwrap_err();
        assert_eq!((err.op, err.path.as_path()), ("chown", root));
        assert_eq!(err.source.raw_os_error(), Some(libc::EPERM), "{err:?}");
    }

    #[test]
    fn foreign_directories_are_reported_and_not_descended() {
        // With the caller's uid as the "old" uid that now belongs to another
        // account (old_uid None), its subdirectory is foreign: reported once,
        // its contents not looked at, and nothing changed.
        // SAFETY: geteuid has no preconditions.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let state = tempfile::tempdir().unwrap();
        let root = state.path();
        fs::create_dir_all(root.join("bob/deeper")).unwrap();
        fs::write(root.join("bob/deeper/file"), "x").unwrap();
        let (uid, gid) = owner(root);
        let c_root = cstring(root.as_os_str()).unwrap();
        let fd = openat(libc::AT_FDCWD, &c_root, DIR_FLAGS, 0).unwrap();
        let st = fstat(fd.as_raw_fd()).unwrap();
        assert_eq!(
            decide(&st, None, uid.wrapping_add(1)),
            Action::SkipForeignOwner {
                owner: uid,
                links: st.st_nlink as u64
            }
        );
        let err = chown_tree_fd(fd, root, uid.wrapping_add(1), gid, None).unwrap_err();
        assert_eq!(err.op, "chown");
        assert_eq!(err.path, root);
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
        let rules = Rules {
            old_uid: None,
            uid: 0,
            gid: 0,
        };
        let e = chown_entry(
            fd.as_raw_fd(),
            &name,
            Path::new("vanished"),
            libc::S_IFREG,
            &rules,
            &mut ByDescriptor,
        )
        .unwrap_err();
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

    /// Records every change and fails the one for `fail_at`, with `EIO`
    /// (an error that stops the walk); the others are applied.
    struct Recorder {
        calls: Vec<PathBuf>,
        fail_at: Option<PathBuf>,
    }

    impl Chown for Recorder {
        fn chown(&mut self, fd: RawFd, path: &Path, uid: u32, gid: u32) -> io::Result<()> {
            self.calls.push(path.to_path_buf());
            if self.fail_at.as_deref() == Some(path) {
                return Err(io::Error::from_raw_os_error(libc::EIO));
            }
            ByDescriptor.chown(fd, path, uid, gid)
        }
    }

    fn walk_with(root: &Path, fail_at: Option<PathBuf>) -> (Recorder, Result<(), WalkError>) {
        let (uid, gid) = owner(root);
        let c_root = cstring(root.as_os_str()).unwrap();
        let fd = openat(libc::AT_FDCWD, &c_root, DIR_FLAGS, 0).unwrap();
        let mut recorder = Recorder {
            calls: Vec::new(),
            fail_at,
        };
        let result = chown_tree_fd_with(fd, root, uid, gid, Some(uid), &mut recorder);
        (recorder, result.map(drop))
    }

    #[test]
    fn the_root_is_changed_last_and_not_at_all_after_a_failure() {
        let state = tempfile::tempdir().unwrap();
        let root = state.path();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/file"), "x").unwrap();
        fs::write(root.join("top"), "x").unwrap();

        let (recorder, result) = walk_with(root, None);
        result.unwrap();
        let mut sorted = recorder.calls.clone();
        sorted.sort();
        let expected: Vec<PathBuf> = ["", "a", "a/b", "a/b/file", "top"]
            .iter()
            .map(|p| {
                if p.is_empty() {
                    root.to_path_buf()
                } else {
                    root.join(p)
                }
            })
            .collect();
        assert_eq!(sorted, expected);
        assert_eq!(recorder.calls.last(), Some(&root.to_path_buf()));
        assert_eq!(recorder.calls.iter().filter(|p| *p == root).count(), 1);

        // A failure in the middle of the walk stops it before the root.
        let (recorder, result) = walk_with(root, Some(root.join("a/b/file")));
        let err = result.unwrap_err();
        assert_eq!(
            (err.op, err.path.as_path()),
            ("chown", root.join("a/b/file").as_path())
        );
        assert!(
            !recorder.calls.iter().any(|p| p == root),
            "{:?}",
            recorder.calls
        );
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
