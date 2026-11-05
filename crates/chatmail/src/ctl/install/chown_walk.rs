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
//! therefore never resolves a path below the root:
//!
//! - each directory is opened relative to its parent's descriptor with
//!   `O_NOFOLLOW | O_DIRECTORY`, so a link swapped in fails to open (`ELOOP` or
//!   `ENOTDIR`) instead of being followed, and is changed with `fchown` on the
//!   opened descriptor;
//! - each entry is classified with `fstatat(AT_SYMLINK_NOFOLLOW)`. Symbolic links
//!   are skipped, as before; other entries are changed with
//!   `fchownat(AT_SYMLINK_NOFOLLOW)`, which never follows a link even when the
//!   entry was replaced by one after the check.
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

/// Directory levels below the root that are still changed. Install-created
/// state (databases, DKIM keys, mail directories) is a few levels deep.
pub const MAX_DEPTH: usize = 64;

const DIR_FLAGS: libc::c_int =
    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

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

struct Frame {
    fd: OwnedFd,
    path: PathBuf,
    names: std::vec::IntoIter<CString>,
    depth: usize,
}

/// Change the owner of `root` and everything below it to `uid:gid` without
/// following symbolic links. `root` is opened as given with `O_NOFOLLOW`, so the
/// caller resolves an operator-chosen root link first.
pub fn chown_tree(root: &Path, uid: u32, gid: u32) -> Result<Vec<Skipped>, WalkError> {
    let fail = |op, path: &Path, source| WalkError {
        op,
        path: path.to_path_buf(),
        source,
    };
    let c_root = cstring(root.as_os_str()).map_err(|e| fail("open", root, e))?;
    let fd = open_dir(None, &c_root).map_err(|e| fail("open", root, e))?;
    fchown(&fd, uid, gid).map_err(|e| fail("chown", root, e))?;
    let names = read_names(&fd).map_err(|e| fail("read_dir", root, e))?;

    let mut skipped = Vec::new();
    let mut stack = vec![Frame {
        fd,
        path: root.to_path_buf(),
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
        match st.st_mode & libc::S_IFMT {
            // Links planted by the service account are never followed or changed.
            libc::S_IFLNK => {}
            libc::S_IFDIR => {
                if frame.depth + 1 > MAX_DEPTH {
                    skip(format!("deeper than {MAX_DEPTH} levels"));
                    continue;
                }
                let child = match open_dir(Some(frame.fd.as_raw_fd()), &name) {
                    Ok(fd) => fd,
                    Err(e) if changed_under_us(&e) => {
                        skip(format!("changed during the walk ({e})"));
                        continue;
                    }
                    Err(e) => return Err(fail("open", &path, e)),
                };
                fchown(&child, uid, gid).map_err(|e| fail("chown", &path, e))?;
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
            _ => {
                // A second name for a file someone else owns: with
                // fs.protected_hardlinks disabled the service account could link
                // a root-owned file into its tree, so do not hand it over.
                if st.st_nlink > 1 && st.st_uid != uid {
                    skip(format!(
                        "has {} hard links and another owner (uid {})",
                        st.st_nlink, st.st_uid
                    ));
                    continue;
                }
                match fchownat_nofollow(frame.fd.as_raw_fd(), &name, uid, gid) {
                    Ok(()) => {}
                    Err(e) if changed_under_us(&e) => {
                        skip(format!("changed during the walk ({e})"));
                    }
                    Err(e) => return Err(fail("chown", &path, e)),
                }
            }
        }
    }
    Ok(skipped)
}

/// Errors that mean an entry was replaced or removed after it was listed.
fn changed_under_us(e: &io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(libc::ELOOP) | Some(libc::ENOTDIR) | Some(libc::ENOENT)
    )
}

fn cstring(s: &OsStr) -> io::Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}

/// Open a directory without following a link in its last component: relative
/// to `parent` when given, otherwise `name` is a path.
fn open_dir(parent: Option<RawFd>, name: &CStr) -> io::Result<OwnedFd> {
    let base = parent.unwrap_or(libc::AT_FDCWD);
    // SAFETY: `name` is a valid NUL-terminated string and `base` is either
    // AT_FDCWD or a descriptor owned by the caller for the duration of the call.
    let fd = unsafe { libc::openat(base, name.as_ptr(), DIR_FLAGS) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` was just returned by openat and is owned by nobody else.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn fchown(fd: &OwnedFd, uid: u32, gid: u32) -> io::Result<()> {
    // SAFETY: `fd` is an open descriptor for the duration of the call.
    if unsafe { libc::fchown(fd.as_raw_fd(), uid, gid) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn fstatat_nofollow(dir: RawFd, name: &CStr) -> io::Result<libc::stat> {
    // SAFETY: `stat` is plain old data; an all-zero value is valid and is
    // overwritten by fstatat on success.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `dir` is an open directory descriptor, `name` is NUL-terminated and
    // `st` is a valid, writable `stat`.
    if unsafe { libc::fstatat(dir, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(st)
}

fn fchownat_nofollow(dir: RawFd, name: &CStr, uid: u32, gid: u32) -> io::Result<()> {
    // SAFETY: `dir` is an open directory descriptor and `name` is NUL-terminated.
    let rc = unsafe { libc::fchownat(dir, name.as_ptr(), uid, gid, libc::AT_SYMLINK_NOFOLLOW) };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
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
        let fd = open_dir(None, &cstring(state.path().as_os_str()).unwrap()).unwrap();
        let name = CString::new("vanished").unwrap();
        let e = fstatat_nofollow(fd.as_raw_fd(), &name).unwrap_err();
        assert!(changed_under_us(&e));
        fs::write(state.path().join("file"), "x").unwrap();
        let e = open_dir(Some(fd.as_raw_fd()), &CString::new("file").unwrap()).unwrap_err();
        assert!(changed_under_us(&e), "{e:?}");
        symlink(state.path(), state.path().join("link")).unwrap();
        let e = open_dir(Some(fd.as_raw_fd()), &CString::new("link").unwrap()).unwrap_err();
        assert!(changed_under_us(&e), "{e:?}");
    }

    #[test]
    fn lists_names_without_dot_entries() {
        let state = tempfile::tempdir().unwrap();
        fs::write(state.path().join("one"), "x").unwrap();
        fs::create_dir(state.path().join("two")).unwrap();
        let fd = open_dir(None, &cstring(state.path().as_os_str()).unwrap()).unwrap();
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
