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
//! madmail alone ("dedicated"), which well-known directories are never
//! changed or removed as a whole, and how paths that another account may have
//! named are shown to the operator.

#![cfg_attr(windows, allow(dead_code))]

use std::path::{Component, Path, PathBuf};

/// Well-known directories that install never re-owns or re-modes (as a config,
/// certificate or state directory) and uninstall never removes as a whole,
/// whatever a flag, a unit or a config file says. See [`system_dir_reason`]
/// for the full rule, which also covers top-level directories and the
/// directories directly inside `/home` and `/Users`.
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

/// Why `path` (normalized) must never be used as a state directory or removed
/// as a whole, or `None`: one of [`DENIED_DIRS`] ("well-known system
/// directory"), a directory directly inside `/` ("top-level directory", such as
/// `/data` or a mount point that may hold other software), or a directory
/// directly inside `/home` or `/Users` ("user home directory").
pub fn system_dir_reason(path: &Path) -> Option<&'static str> {
    let normalized = normalize(path);
    if is_denied_dir(&normalized) {
        return Some("well-known system directory");
    }
    match normalized.parent() {
        Some(p) if p == Path::new("/") => Some("top-level directory"),
        Some(p) if p == Path::new("/home") || p == Path::new("/Users") => {
            Some("user home directory")
        }
        _ => None,
    }
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

/// A path for human output (terminal text, notes, warnings and error
/// messages) when part of it may come from another account: a name in the
/// state tree, a walk entry or a link target. Control characters (C0, DEL and
/// C1), other characters that change how a terminal shows text (line and
/// paragraph separators, bidirectional overrides), a backslash, and bytes that
/// are not UTF-8 are written as visible escapes (`\n`, `\r`, `\t`, `\\`,
/// `\xNN` for other ASCII controls and for bytes that are not UTF-8,
/// `\u{NNNN}` for the others), so a file name cannot start a new line or send a
/// terminal escape sequence.
pub fn shown(path: &Path) -> String {
    escape_bytes(path_bytes(path).as_ref())
}

#[cfg(unix)]
fn path_bytes(path: &Path) -> std::borrow::Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;
    std::borrow::Cow::Borrowed(path.as_os_str().as_bytes())
}

#[cfg(not(unix))]
fn path_bytes(path: &Path) -> std::borrow::Cow<'_, [u8]> {
    match path.to_string_lossy() {
        std::borrow::Cow::Borrowed(s) => std::borrow::Cow::Borrowed(s.as_bytes()),
        std::borrow::Cow::Owned(s) => std::borrow::Cow::Owned(s.into_bytes()),
    }
}

/// Characters [`shown`] escapes besides the backslash.
fn needs_escape(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{2028}' | '\u{2029}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}' | '\u{feff}'
        )
}

/// [`shown`] for any byte string.
pub fn escape_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if needs_escape(c) && c.is_ascii() => {
                    let _ = write!(out, "\\x{:02x}", c as u32);
                }
                c if needs_escape(c) => {
                    let _ = write!(out, "\\u{{{:04x}}}", c as u32);
                }
                c => out.push(c),
            }
        }
        for b in chunk.invalid() {
            let _ = write!(out, "\\x{b:02x}");
        }
    }
    out
}

/// `path` quoted for a POSIX shell command shown to the operator. A path of
/// plain characters is shown as it is; one with a space, quote or other shell
/// character in single quotes; one with a control or other character that
/// [`shown`] escapes, or with bytes that are not UTF-8, in bash `$'...'`
/// quoting with every such byte as `\xNN`, so the command names the same bytes
/// and the terminal shows no raw control character.
pub fn shell_quote(path: &Path) -> String {
    let bytes = path_bytes(path);
    let bytes = bytes.as_ref();
    let plain = std::str::from_utf8(bytes)
        .ok()
        .filter(|s| !s.chars().any(needs_escape));
    match plain {
        Some(text)
            if !text.is_empty()
                && text
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/._-+:@".contains(&b)) =>
        {
            text.to_string()
        }
        Some(text) => format!("'{}'", text.replace('\'', r"'\''")),
        None => {
            use std::fmt::Write;
            let mut out = String::from("$'");
            for &b in bytes {
                match b {
                    b'\'' => out.push_str("\\'"),
                    b'\\' => out.push_str("\\\\"),
                    0x20..=0x7e => out.push(b as char),
                    _ => {
                        let _ = write!(out, "\\x{b:02x}");
                    }
                }
            }
            out.push('\'');
            out
        }
    }
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

    #[test]
    fn system_dirs_cover_denied_top_level_and_home_dirs() {
        for (dir, why) in [
            ("/", "well-known system directory"),
            ("/var/lib", "well-known system directory"),
            ("/var/lib/madmail/..", "well-known system directory"),
            ("/etc/", "well-known system directory"),
            ("/data", "top-level directory"),
            ("/madmail", "top-level directory"),
            ("/home/alice", "user home directory"),
            ("/Users/alice/", "user home directory"),
        ] {
            assert_eq!(system_dir_reason(Path::new(dir)), Some(why), "{dir}");
        }
        for dir in [
            "/var/lib/madmail",
            "/srv/madmail",
            "/home/alice/madmail",
            "/data/x",
        ] {
            assert_eq!(system_dir_reason(Path::new(dir)), None, "{dir}");
        }
    }

    fn has_raw_control(s: &str) -> bool {
        s.chars().any(needs_escape)
    }

    #[cfg(unix)]
    #[test]
    fn shown_and_quoted_paths_have_no_raw_control_bytes() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let p = |b: &[u8]| PathBuf::from(OsStr::from_bytes(b));
        assert_eq!(
            shown(Path::new("/var/lib/madmail/a b")),
            "/var/lib/madmail/a b"
        );
        assert_eq!(shown(&p(b"/s/x\nrun: rm -rf /")), "/s/x\\nrun: rm -rf /");
        assert_eq!(shown(&p(b"/s/a\rb\tc")), "/s/a\\rb\\tc");
        assert_eq!(shown(&p(b"/s/\x1b[2Kfake")), "/s/\\x1b[2Kfake");
        assert_eq!(shown(&p(b"/s/\xff\xfeok")), "/s/\\xff\\xfeok");
        assert_eq!(
            shown(&p("/s/\u{9b}2K\u{202e}".as_bytes())),
            "/s/\\u{009b}2K\\u{202e}"
        );
        assert_eq!(shown(&p(b"/s/back\\slash")), "/s/back\\\\slash");
        for raw in [
            &b"/s/x\nchown -h root /etc"[..],
            b"/s/a\rb",
            b"/s/\x1b[2K",
            b"/s/it's\x07",
            b"/s/\xff",
            b"/s/\x7f",
        ] {
            for out in [shown(&p(raw)), shell_quote(&p(raw))] {
                assert!(!has_raw_control(&out), "{out:?}");
            }
        }

        assert_eq!(
            shell_quote(Path::new("/var/lib/madmail/x.db")),
            "/var/lib/madmail/x.db"
        );
        assert_eq!(shell_quote(Path::new("/s/a b")), "'/s/a b'");
        assert_eq!(shell_quote(Path::new("/s/it's")), r"'/s/it'\''s'");
        assert_eq!(shell_quote(&p(b"/s/x\ny")), r"$'/s/x\x0ay'");
        assert_eq!(shell_quote(&p(b"/s/\x1b[2K")), r"$'/s/\x1b[2K'");
        assert_eq!(shell_quote(&p(b"/s/it's\r")), r"$'/s/it\'s\x0d'");
        assert_eq!(shell_quote(&p(b"/s/\xff\\")), r"$'/s/\xff\\'");
    }
}
