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

//! Naming rules shared by install and uninstall: which directories belong to
//! madmail alone ("dedicated") and which well-known directories are never
//! changed or removed as a whole.

#![cfg_attr(windows, allow(dead_code))]

use std::path::{Component, Path, PathBuf};

/// Well-known directories that install never re-owns or re-modes and uninstall
/// never removes as a whole, whatever a flag, a unit or a config file says.
pub const DENIED_DIRS: &[&str] = &[
    "/",
    "/bin",
    "/boot",
    "/dev",
    "/etc",
    "/home",
    "/lib",
    "/lib64",
    "/media",
    "/mnt",
    "/opt",
    "/proc",
    "/root",
    "/run",
    "/sbin",
    "/srv",
    "/sys",
    "/tmp",
    "/Users",
    "/usr",
    "/usr/bin",
    "/usr/lib",
    "/usr/local",
    "/usr/local/bin",
    "/usr/local/etc",
    "/usr/local/lib",
    "/usr/local/share",
    "/usr/sbin",
    "/usr/share",
    "/var",
    "/var/cache",
    "/var/lib",
    "/var/log",
    "/var/mail",
    "/var/spool",
    "/var/tmp",
];

/// Names install and uninstall treat as madmail's own: `madmail`, `chatmail`,
/// and names starting with `madmail-`, `madmail_`, `chatmail-` or `chatmail_`.
pub fn is_family_stem(stem: &str) -> bool {
    stem == "madmail"
        || stem == "chatmail"
        || stem.starts_with("madmail-")
        || stem.starts_with("madmail_")
        || stem.starts_with("chatmail-")
        || stem.starts_with("chatmail_")
}

/// Lexically resolve `.` and `..` and drop trailing slashes.
pub fn normalize(path: &Path) -> PathBuf {
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

/// Whether `path` (normalized) is one of [`DENIED_DIRS`].
pub fn is_denied_dir(path: &Path) -> bool {
    let normalized = normalize(path);
    DENIED_DIRS.iter().any(|d| normalized == Path::new(d))
}

/// Whether the directory with this device and inode is one of [`DENIED_DIRS`]
/// (each looked up with `stat`, so `/bin` matches `/usr/bin` where it is a
/// link). Catches an operator link with a madmail name that leads to one.
#[cfg(unix)]
pub fn is_denied_dir_id(id: (u64, u64)) -> bool {
    use std::os::unix::fs::MetadataExt;
    DENIED_DIRS.iter().any(|d| {
        std::fs::metadata(d)
            .map(|m| (m.dev(), m.ino()) == id)
            .unwrap_or(false)
    })
}

/// Whether the last component of `path` (normalized) is a family name or the
/// binary name `primary`.
pub fn is_named_for_madmail(path: &Path, primary: &str) -> bool {
    normalize(path)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| is_family_stem(name) || name == primary)
}

/// A directory install may re-own and re-mode: named for madmail and not on the
/// deny list, or a `certs` directory directly inside such a directory (the
/// default `/etc/madmail/certs`).
pub fn is_dedicated_dir(path: &Path, primary: &str) -> bool {
    let path = normalize(path);
    if is_denied_dir(&path) {
        return false;
    }
    if is_named_for_madmail(&path, primary) {
        return true;
    }
    path.file_name().is_some_and(|n| n == "certs")
        && path
            .parent()
            .is_some_and(|parent| !is_denied_dir(parent) && is_named_for_madmail(parent, primary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedicated_dirs_are_named_for_madmail_and_not_denied() {
        for dir in [
            "/etc/madmail",
            "/etc/madmail/",
            "/var/lib/chatmail-test",
            "/opt/mymail",
            "/etc/madmail/certs",
            "/srv/madmail_2/certs",
        ] {
            assert!(is_dedicated_dir(Path::new(dir), "mymail"), "{dir}");
        }
        for dir in [
            "/",
            "/etc",
            "/usr/local/etc",
            "/var/lib",
            "/opt",
            "/srv/data",
            "/etc/certs",
            "/usr/local/etc/certs",
            "/etc/madmail/certs/extra",
            "/etc/madmailx",
        ] {
            assert!(!is_dedicated_dir(Path::new(dir), "mymail"), "{dir}");
        }
        assert!(is_denied_dir(Path::new("/usr/local/etc/")));
        assert!(is_denied_dir(Path::new("/var/lib/madmail/../..")));
    }
}
