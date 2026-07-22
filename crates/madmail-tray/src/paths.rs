// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Default config / state / binary locations for the tray helper.

use std::path::{Path, PathBuf};

/// Resolve default Madmail layout (Windows ProgramData, Unix FHS-ish for smoke tests).
pub fn default_layout() -> (PathBuf, PathBuf) {
    #[cfg(windows)]
    {
        let root = std::env::var_os("PROGRAMDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join("Madmail");
        (root.join("config").join("madmail.conf"), root.join("data"))
    }
    #[cfg(not(windows))]
    {
        (
            PathBuf::from("/etc/madmail/madmail.conf"),
            PathBuf::from("/var/lib/madmail"),
        )
    }
}

/// Prefer sibling `madmail` / `madmail.exe` next to this process, else PATH name.
pub fn madmail_binary() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            #[cfg(windows)]
            let candidate = dir.join("madmail.exe");
            #[cfg(not(windows))]
            let candidate = dir.join("madmail");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    #[cfg(windows)]
    {
        PathBuf::from("madmail.exe")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("madmail")
    }
}

pub fn admin_token_path(state_dir: &Path) -> PathBuf {
    state_dir.join("admin_token")
}

pub fn read_admin_token(state_dir: &Path) -> Option<String> {
    let p = admin_token_path(state_dir);
    std::fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Best-effort admin URL from config hostname, else localhost HTTP.
pub fn admin_url(config_path: &Path) -> String {
    if config_path.is_file() {
        if let Ok(cfg) = chatmail_config::load_config(config_path) {
            let host = cfg
                .hostname
                .or(cfg.primary_domain)
                .unwrap_or_else(|| "127.0.0.1".into());
            let host = host.trim_matches(|c| c == '[' || c == ']').to_string();
            let path = cfg.admin_path.unwrap_or_else(|| "/api/admin".into());
            let path = if path.starts_with('/') {
                path
            } else {
                format!("/{path}")
            };
            // Prefer HTTPS 443 style; operators can fix if using custom ports.
            if host == "127.0.0.1" || host == "localhost" {
                return format!("http://{host}:8080{path}");
            }
            return format!("https://{host}{path}");
        }
    }
    "http://127.0.0.1:8080/api/admin".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_token_roundtrip_path() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_admin_token(dir.path()).is_none());
        std::fs::write(dir.path().join("admin_token"), "abc\n").unwrap();
        assert_eq!(read_admin_token(dir.path()).as_deref(), Some("abc"));
    }

    #[test]
    fn admin_url_defaults() {
        let u = admin_url(Path::new("/nonexistent/madmail.conf"));
        assert!(u.contains("127.0.0.1"), "{u}");
    }
}
