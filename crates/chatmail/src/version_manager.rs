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

//! Versioned install tree under a platform install root (TDD `24-version-manager.md`).
//!
//! - Unix default root: `/opt/madmail`
//! - Windows default root: `%ProgramFiles%\Madmail`
//! - Override: `MADMAIL_INSTALL_ROOT` (Unix alias: `MADMAIL_OPT_ROOT`)

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chatmail_types::{ChatmailError, Result};
use serde::{Deserialize, Serialize};

/// Env var for install root (tests and custom layouts).
pub const ENV_INSTALL_ROOT: &str = "MADMAIL_INSTALL_ROOT";
/// Deprecated Unix alias for [`ENV_INSTALL_ROOT`].
pub const ENV_OPT_ROOT_ALIAS: &str = "MADMAIL_OPT_ROOT";

const DEFAULT_KEEP: usize = 5;

/// Default platform install root (no env override).
pub fn default_install_root() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"))
            .join("Madmail")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/opt/madmail")
    }
}

/// Resolve install root: `MADMAIL_INSTALL_ROOT`, else Unix `MADMAIL_OPT_ROOT`, else default.
pub fn install_root() -> PathBuf {
    if let Ok(p) = std::env::var(ENV_INSTALL_ROOT) {
        let t = p.trim();
        if !t.is_empty() {
            return PathBuf::from(t);
        }
    }
    #[cfg(not(windows))]
    if let Ok(p) = std::env::var(ENV_OPT_ROOT_ALIAS) {
        let t = p.trim();
        if !t.is_empty() {
            return PathBuf::from(t);
        }
    }
    default_install_root()
}

/// Executable file name inside a version directory.
pub fn binary_file_name() -> &'static str {
    #[cfg(windows)]
    {
        "madmail.exe"
    }
    #[cfg(not(windows))]
    {
        "madmail"
    }
}

/// Stable PATH entry path (symlink/junction target side).
pub fn default_stable_binary_path() -> PathBuf {
    #[cfg(windows)]
    {
        install_root().join("bin").join(binary_file_name())
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/usr/local/bin/madmail")
    }
}

pub fn versions_dir(root: &Path) -> PathBuf {
    root.join("versions")
}

pub fn version_dir(root: &Path, version_id: &str) -> PathBuf {
    versions_dir(root).join(version_id)
}

pub fn version_binary_path(root: &Path, version_id: &str) -> PathBuf {
    version_dir(root, version_id).join(binary_file_name())
}

pub fn current_link_path(root: &Path) -> PathBuf {
    root.join("current")
}

/// Reject path separators and invalid version directory names.
pub fn sanitize_version_id(raw: &str) -> Result<String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(ChatmailError::config("version id is empty"));
    }
    if s.contains('/') || s.contains('\\') || s.contains("..") {
        return Err(ChatmailError::config(format!(
            "invalid version id (path separators not allowed): {s}"
        )));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
    {
        return Err(ChatmailError::config(format!(
            "invalid version id (allowed [0-9A-Za-z._+-]): {s}"
        )));
    }
    // Windows reserved device names
    let upper = s.to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "LPT1"
    ) {
        return Err(ChatmailError::config(format!(
            "invalid version id (reserved name): {s}"
        )));
    }
    Ok(s.to_string())
}

/// Parse a version id from `madmail version` stdout (e.g. `madmail-v2 2.20.1`).
pub fn parse_version_from_preflight_output(stdout: &str) -> String {
    let first = stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    // Prefer last whitespace-separated token that looks like semver-ish
    for tok in first.split_whitespace().rev() {
        if tok.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            if let Ok(id) = sanitize_version_id(tok) {
                return id;
            }
        }
    }
    // Fallback: whole first line sanitized or timestamp
    if let Ok(id) = sanitize_version_id(&first.replace(' ', "-")) {
        return id;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unknown-{ts}")
}

/// GitHub Releases latest download URL for this host (no network).
pub fn github_latest_asset_url() -> String {
    let asset = host_release_asset_name();
    format!("https://github.com/themadorg/madmail/releases/latest/download/{asset}")
}

/// Asset file name for the running OS/arch.
pub fn host_release_asset_name() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "madmail-linux-amd64.tar.gz"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "madmail-linux-arm64.tar.gz"
    }
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        "madmail-windows-amd64.tar.gz"
    }
    #[cfg(all(windows, target_arch = "aarch64"))]
    {
        "madmail-windows-arm64.tar.gz"
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64"),
        all(windows, target_arch = "aarch64"),
    )))]
    {
        "madmail-linux-amd64.tar.gz"
    }
}

/// GitHub Releases API (list remote metadata).
pub fn github_releases_api_url() -> &'static str {
    "https://api.github.com/repos/themadorg/madmail/releases"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionMeta {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_ok: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstalledVersion {
    pub version: String,
    pub path: PathBuf,
    pub binary: PathBuf,
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<VersionMeta>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VersionListEntry {
    pub version: String,
    /// `local` | `remote` | `both`
    pub source: String,
    pub active: bool,
    pub installed: bool,
    pub remote_latest: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_size: Option<u64>,
}

/// Ensure `{root}/versions` exists.
pub fn ensure_install_layout(root: &Path) -> Result<()> {
    let vdir = versions_dir(root);
    fs::create_dir_all(&vdir)
        .map_err(|e| ChatmailError::config(format!("failed to create {}: {e}", vdir.display())))?;
    Ok(())
}

pub fn write_meta(root: &Path, version_id: &str, meta: &VersionMeta) -> Result<()> {
    let dir = version_dir(root, version_id);
    fs::create_dir_all(&dir)
        .map_err(|e| ChatmailError::config(format!("mkdir {}: {e}", dir.display())))?;
    let path = dir.join("meta.json");
    let json = serde_json::to_vec_pretty(meta)
        .map_err(|e| ChatmailError::config(format!("serialize meta: {e}")))?;
    let mut f = File::create(&path)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", path.display())))?;
    f.write_all(&json)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", path.display())))?;
    Ok(())
}

pub fn read_meta(root: &Path, version_id: &str) -> Result<Option<VersionMeta>> {
    let path = version_dir(root, version_id).join("meta.json");
    if !path.is_file() {
        return Ok(None);
    }
    let data = fs::read(&path)
        .map_err(|e| ChatmailError::config(format!("read {}: {e}", path.display())))?;
    let meta: VersionMeta = serde_json::from_slice(&data)
        .map_err(|e| ChatmailError::config(format!("parse meta {}: {e}", path.display())))?;
    Ok(Some(meta))
}

/// Resolve active version id from `current` symlink or stable path.
pub fn resolve_active_version(root: &Path) -> Result<Option<String>> {
    let current = current_link_path(root);
    if current.exists() || current.symlink_metadata().is_ok() {
        if let Ok(target) = fs::read_link(&current) {
            let target = if target.is_absolute() {
                target
            } else {
                current.parent().unwrap_or(root).join(target)
            };
            if let Some(name) = target.file_name().and_then(|s| s.to_str()) {
                if versions_dir(root).join(name).is_dir() {
                    return Ok(Some(name.to_string()));
                }
            }
            // current might point at versions/X/madmail
            if let Some(parent) = target.parent() {
                if let Some(name) = parent.file_name().and_then(|s| s.to_str()) {
                    if parent
                        .parent()
                        .is_some_and(|p| p == versions_dir(root) || p.ends_with("versions"))
                    {
                        return Ok(Some(name.to_string()));
                    }
                }
            }
        }
    }
    // Fall back: which version binary is the same inode/path as stable?
    let stable = default_stable_binary_path();
    if let Ok(real) = fs::canonicalize(&stable) {
        if let Some(id) = version_id_from_binary_path(root, &real) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn version_id_from_binary_path(root: &Path, binary: &Path) -> Option<String> {
    let versions = versions_dir(root);
    let parent = binary.parent()?;
    if parent.parent()? != versions && !parent.starts_with(&versions) {
        // still try file_name of parent
    }
    let name = parent.file_name()?.to_str()?;
    if version_binary_path(root, name) == *binary
        || fs::canonicalize(version_binary_path(root, name))
            .ok()
            .is_some_and(|p| p == binary)
    {
        return Some(name.to_string());
    }
    if parent.starts_with(&versions) {
        return Some(name.to_string());
    }
    None
}

pub fn list_installed(root: &Path) -> Result<Vec<InstalledVersion>> {
    let vdir = versions_dir(root);
    if !vdir.is_dir() {
        return Ok(Vec::new());
    }
    let active = resolve_active_version(root)?.unwrap_or_default();
    let mut out = Vec::new();
    for ent in fs::read_dir(&vdir)
        .map_err(|e| ChatmailError::config(format!("read {}: {e}", vdir.display())))?
    {
        let ent = ent.map_err(|e| ChatmailError::config(format!("readdir: {e}")))?;
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if !ent.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(id) = sanitize_version_id(&name) else {
            continue;
        };
        let bin = version_binary_path(root, &id);
        if !bin.is_file() {
            continue;
        }
        let meta = read_meta(root, &id).ok().flatten();
        out.push(InstalledVersion {
            version: id.clone(),
            path: version_dir(root, &id),
            binary: bin,
            active: id == active,
            meta,
        });
    }
    out.sort_by(|a, b| b.version.cmp(&a.version));
    Ok(out)
}

/// Copy candidate into the version tree (does not activate). Caller must verify signature first.
pub fn install_candidate(
    root: &Path,
    version_id: &str,
    src: &Path,
    meta: VersionMeta,
) -> Result<PathBuf> {
    let id = sanitize_version_id(version_id)?;
    ensure_install_layout(root)?;
    let dest_dir = version_dir(root, &id);
    fs::create_dir_all(&dest_dir)
        .map_err(|e| ChatmailError::config(format!("mkdir {}: {e}", dest_dir.display())))?;
    let dest = version_binary_path(root, &id);

    // Refuse to follow unexpected symlink dest
    if dest
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(ChatmailError::config(format!(
            "refusing to write through symlink at {}",
            dest.display()
        )));
    }

    let staging = dest_dir.join(format!(".staging-{}", std::process::id()));
    {
        let mut from = File::open(src)
            .map_err(|e| ChatmailError::config(format!("open {}: {e}", src.display())))?;
        let mut to = File::create(&staging)
            .map_err(|e| ChatmailError::config(format!("create {}: {e}", staging.display())))?;
        io::copy(&mut from, &mut to)
            .map_err(|e| ChatmailError::config(format!("copy to staging: {e}")))?;
        to.sync_all()
            .map_err(|e| ChatmailError::config(format!("sync staging: {e}")))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o755))
            .map_err(|e| ChatmailError::config(format!("chmod staging: {e}")))?;
    }
    fs::rename(&staging, &dest).map_err(|e| {
        let _ = fs::remove_file(&staging);
        ChatmailError::config(format!("install binary to {}: {e}", dest.display()))
    })?;
    write_meta(root, &id, &meta)?;
    Ok(dest)
}

/// Point `current` and stable PATH entry at `version_id`.
pub fn set_active(root: &Path, version_id: &str, stable_path: &Path) -> Result<()> {
    let id = sanitize_version_id(version_id)?;
    let bin = version_binary_path(root, &id);
    if !bin.is_file() {
        return Err(ChatmailError::config(format!(
            "version {id} binary not found at {}",
            bin.display()
        )));
    }
    ensure_install_layout(root)?;

    // current -> versions/<id> (directory)
    let current = current_link_path(root);
    let ver_dir = version_dir(root, &id);
    atomic_symlink(&ver_dir, &current)?;

    // stable PATH -> binary
    if let Some(parent) = stable_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| ChatmailError::config(format!("mkdir {}: {e}", parent.display())))?;
    }
    atomic_symlink(&bin, stable_path)?;
    Ok(())
}

fn atomic_symlink(target: &Path, link: &Path) -> Result<()> {
    let parent = link
        .parent()
        .ok_or_else(|| ChatmailError::config("symlink has no parent"))?;
    fs::create_dir_all(parent)
        .map_err(|e| ChatmailError::config(format!("mkdir {}: {e}", parent.display())))?;
    let tmp = parent.join(format!(
        ".madmail-link-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = fs::remove_file(&tmp);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, &tmp).map_err(|e| {
            ChatmailError::config(format!(
                "symlink {} -> {}: {e}",
                tmp.display(),
                target.display()
            ))
        })?;
    }
    #[cfg(windows)]
    {
        // Prefer file symlink when target is a file; dir symlink for directories.
        let res = if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, &tmp)
        } else {
            std::os::windows::fs::symlink_file(target, &tmp)
        };
        res.map_err(|e| {
            ChatmailError::config(format!(
                "symlink {} -> {}: {e} (Administrator or Developer Mode may be required)",
                tmp.display(),
                target.display()
            ))
        })?;
    }
    // Replace existing link/file
    if link.exists() || link.symlink_metadata().is_ok() {
        let _ = fs::remove_file(link);
        // directory junction/symlink
        let _ = fs::remove_dir(link);
    }
    fs::rename(&tmp, link).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        ChatmailError::config(format!(
            "activate link {} -> {}: {e}",
            link.display(),
            target.display()
        ))
    })?;
    Ok(())
}

/// Delete oldest non-active versions beyond `keep` (default 5). Returns removed ids.
pub fn prune(root: &Path, keep: usize) -> Result<Vec<String>> {
    let keep = if keep == 0 { 0 } else { keep };
    let active = resolve_active_version(root)?;
    let mut installed = list_installed(root)?;
    // sort by version string descending already; for prune use mtime of dir
    installed.sort_by(|a, b| {
        let ma = fs::metadata(&a.path).and_then(|m| m.modified()).ok();
        let mb = fs::metadata(&b.path).and_then(|m| m.modified()).ok();
        mb.cmp(&ma)
    });
    let mut removed = Vec::new();
    let mut kept = 0usize;
    for v in &installed {
        if active.as_deref() == Some(v.version.as_str()) {
            continue; // never prune active
        }
        if keep > 0 && kept < keep {
            kept += 1;
            continue;
        }
        remove_version(root, &v.version)?;
        removed.push(v.version.clone());
    }
    Ok(removed)
}

pub fn remove_version(root: &Path, version_id: &str) -> Result<()> {
    let id = sanitize_version_id(version_id)?;
    if resolve_active_version(root)?.as_deref() == Some(id.as_str()) {
        return Err(ChatmailError::config(format!(
            "refusing to remove active version {id}"
        )));
    }
    let dir = version_dir(root, &id);
    if !dir.is_dir() {
        return Err(ChatmailError::config(format!("version {id} not found")));
    }
    fs::remove_dir_all(&dir)
        .map_err(|e| ChatmailError::config(format!("remove {}: {e}", dir.display())))?;
    Ok(())
}

/// Remote release tag metadata (from API or tests).
#[derive(Debug, Clone)]
pub struct RemoteRelease {
    pub version: String,
    pub published_at: Option<String>,
    pub asset: Option<String>,
    pub asset_size: Option<u64>,
    pub is_latest: bool,
}

/// Merge local installs with remote releases for `versions list --remote`.
pub fn merge_local_and_remote(
    local: &[InstalledVersion],
    remote: &[RemoteRelease],
) -> Vec<VersionListEntry> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, VersionListEntry> = BTreeMap::new();

    for l in local {
        map.insert(
            l.version.clone(),
            VersionListEntry {
                version: l.version.clone(),
                source: "local".into(),
                active: l.active,
                installed: true,
                remote_latest: false,
                published_at: None,
                asset: None,
                asset_size: None,
            },
        );
    }

    for r in remote {
        let e = map
            .entry(r.version.clone())
            .or_insert_with(|| VersionListEntry {
                version: r.version.clone(),
                source: "remote".into(),
                active: false,
                installed: false,
                remote_latest: false,
                published_at: None,
                asset: None,
                asset_size: None,
            });
        if e.installed {
            e.source = "both".into();
        } else {
            e.source = "remote".into();
        }
        e.remote_latest = r.is_latest;
        e.published_at = r.published_at.clone();
        e.asset = r.asset.clone();
        e.asset_size = r.asset_size;
    }

    let mut out: Vec<_> = map.into_values().collect();
    out.sort_by(|a, b| b.version.cmp(&a.version));
    out
}

/// Strip leading `v` from GitHub tag names.
pub fn normalize_release_tag(tag: &str) -> String {
    let t = tag.trim();
    t.strip_prefix('v').unwrap_or(t).to_string()
}

/// Parse a minimal subset of GitHub releases JSON into [`RemoteRelease`]s.
pub fn parse_github_releases_json(
    body: &str,
    host_asset_substr: &str,
) -> Result<Vec<RemoteRelease>> {
    let val: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| ChatmailError::config(format!("github releases json: {e}")))?;
    let arr = val
        .as_array()
        .ok_or_else(|| ChatmailError::config("github releases: expected array"))?;
    let mut out = Vec::new();
    let mut first = true;
    for item in arr {
        if item.get("draft").and_then(|d| d.as_bool()).unwrap_or(false) {
            continue;
        }
        if item
            .get("prerelease")
            .and_then(|d| d.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let tag = item
            .get("tag_name")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        if tag.is_empty() {
            continue;
        }
        let version = normalize_release_tag(&tag);
        let Ok(version) = sanitize_version_id(&version) else {
            continue;
        };
        let published_at = item
            .get("published_at")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
        let mut asset = None;
        let mut asset_size = None;
        if let Some(assets) = item.get("assets").and_then(|a| a.as_array()) {
            for a in assets {
                let name = a.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if name.contains(host_asset_substr)
                    || name == host_release_asset_name()
                    || name.contains("madmail")
                {
                    // Prefer exact host asset
                    if name == host_release_asset_name() || name.contains(host_asset_substr) {
                        asset = Some(name.to_string());
                        asset_size = a.get("size").and_then(|s| s.as_u64());
                        if name == host_release_asset_name() {
                            break;
                        }
                    }
                }
            }
        }
        out.push(RemoteRelease {
            version,
            published_at,
            asset,
            asset_size,
            is_latest: first,
        });
        first = false;
        if out.len() >= 20 {
            break;
        }
    }
    Ok(out)
}

/// Fetch remote releases (network). Used by `versions list --remote`.
pub fn fetch_remote_releases(accept_invalid_certs: bool) -> Result<Vec<RemoteRelease>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("madmail-version-manager")
        .danger_accept_invalid_certs(accept_invalid_certs)
        .build()
        .map_err(|e| ChatmailError::config(format!("http client: {e}")))?;
    let resp = client
        .get(github_releases_api_url())
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| ChatmailError::config(format!("github releases fetch failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(ChatmailError::config(format!(
            "github releases HTTP {}",
            resp.status()
        )));
    }
    let body = resp
        .text()
        .map_err(|e| ChatmailError::config(format!("github releases body: {e}")))?;
    let substr = if cfg!(windows) { "windows" } else { "linux" };
    parse_github_releases_json(&body, substr)
}

pub fn default_keep() -> usize {
    DEFAULT_KEEP
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_fake_bin(path: &Path) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        let mut f = File::create(path).unwrap();
        f.write_all(b"#!/bin/sh\necho madmail-v2 2.20.0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn sanitize_rejects_path_separators() {
        assert!(sanitize_version_id("../x").is_err());
        assert!(sanitize_version_id("a/b").is_err());
        assert!(sanitize_version_id("a\\b").is_err());
        assert!(sanitize_version_id("2.20.0").is_ok());
        assert!(sanitize_version_id("2.20.0+git.abc").is_ok());
    }

    #[test]
    fn parse_version_from_preflight() {
        assert_eq!(
            parse_version_from_preflight_output("madmail-v2 2.20.1\n"),
            "2.20.1"
        );
        assert_eq!(parse_version_from_preflight_output("2.19.0"), "2.19.0");
    }

    #[test]
    fn default_root_is_platform_specific() {
        #[cfg(windows)]
        {
            let r = default_install_root();
            assert!(r.to_string_lossy().contains("Madmail"));
            assert!(!r.to_string_lossy().contains("/opt"));
        }
        #[cfg(not(windows))]
        {
            assert_eq!(default_install_root(), PathBuf::from("/opt/madmail"));
        }
    }

    #[test]
    fn install_root_env_override() {
        let dir = TempDir::new().unwrap();
        // SAFETY: test-only env mutation; serial tests in this module.
        std::env::set_var(ENV_INSTALL_ROOT, dir.path());
        assert_eq!(install_root(), dir.path());
        std::env::remove_var(ENV_INSTALL_ROOT);
    }

    #[test]
    fn github_latest_url_uses_releases_latest_download() {
        let url = github_latest_asset_url();
        assert!(url.starts_with("https://github.com/themadorg/madmail/releases/latest/download/"));
        assert!(url.contains("madmail"));
    }

    #[test]
    fn install_list_activate_prune() {
        let root = TempDir::new().unwrap();
        let stable = root.path().join("bin").join(binary_file_name());
        let src1 = root.path().join("payload1");
        write_fake_bin(&src1);

        install_candidate(
            root.path(),
            "2.19.0",
            &src1,
            VersionMeta {
                version: "2.19.0".into(),
                installed_at: None,
                source: Some("test".into()),
                source_url: None,
                sha256: None,
                variant: None,
                os: None,
                signature_ok: Some(true),
            },
        )
        .unwrap();
        install_candidate(
            root.path(),
            "2.20.0",
            &src1,
            VersionMeta {
                version: "2.20.0".into(),
                installed_at: None,
                source: Some("test".into()),
                source_url: None,
                sha256: None,
                variant: None,
                os: None,
                signature_ok: Some(true),
            },
        )
        .unwrap();
        install_candidate(
            root.path(),
            "2.18.0",
            &src1,
            VersionMeta {
                version: "2.18.0".into(),
                installed_at: None,
                source: Some("test".into()),
                source_url: None,
                sha256: None,
                variant: None,
                os: None,
                signature_ok: Some(true),
            },
        )
        .unwrap();

        set_active(root.path(), "2.20.0", &stable).unwrap();
        assert_eq!(
            resolve_active_version(root.path()).unwrap().as_deref(),
            Some("2.20.0")
        );
        let list = list_installed(root.path()).unwrap();
        assert_eq!(list.len(), 3);
        assert!(list.iter().any(|v| v.active && v.version == "2.20.0"));

        // keep 1 non-active → remove one of the two non-active
        let removed = prune(root.path(), 1).unwrap();
        assert_eq!(removed.len(), 1);
        assert!(!removed.contains(&"2.20.0".to_string()));
        assert!(list_installed(root.path()).unwrap().len() >= 2);

        // cannot remove active
        assert!(remove_version(root.path(), "2.20.0").is_err());
    }

    #[test]
    fn merge_local_remote_marks_sources() {
        let local = vec![InstalledVersion {
            version: "2.20.0".into(),
            path: PathBuf::from("/x"),
            binary: PathBuf::from("/x/b"),
            active: true,
            meta: None,
        }];
        let remote = vec![
            RemoteRelease {
                version: "2.20.1".into(),
                published_at: Some("2026-01-01T00:00:00Z".into()),
                asset: Some("madmail-linux-amd64.tar.gz".into()),
                asset_size: Some(100),
                is_latest: true,
            },
            RemoteRelease {
                version: "2.20.0".into(),
                published_at: None,
                asset: None,
                asset_size: None,
                is_latest: false,
            },
        ];
        let merged = merge_local_and_remote(&local, &remote);
        let v201 = merged.iter().find(|e| e.version == "2.20.1").unwrap();
        assert_eq!(v201.source, "remote");
        assert!(!v201.installed);
        assert!(v201.remote_latest);
        let v200 = merged.iter().find(|e| e.version == "2.20.0").unwrap();
        assert_eq!(v200.source, "both");
        assert!(v200.installed);
        assert!(v200.active);
    }

    #[test]
    fn parse_github_releases_json_basic() {
        let body = r#"[
          {"tag_name":"v2.20.1","draft":false,"prerelease":false,
           "published_at":"2026-08-01T00:00:00Z",
           "assets":[{"name":"madmail-linux-amd64.tar.gz","size":123}]},
          {"tag_name":"v2.20.0","draft":false,"prerelease":false,"assets":[]}
        ]"#;
        let rels = parse_github_releases_json(body, "linux").unwrap();
        assert_eq!(rels[0].version, "2.20.1");
        assert!(rels[0].is_latest);
        assert_eq!(rels[0].asset.as_deref(), Some("madmail-linux-amd64.tar.gz"));
        assert_eq!(rels[1].version, "2.20.0");
    }

    #[test]
    fn meta_roundtrip() {
        let root = TempDir::new().unwrap();
        let src = root.path().join("b");
        write_fake_bin(&src);
        install_candidate(
            root.path(),
            "1.0.0",
            &src,
            VersionMeta {
                version: "1.0.0".into(),
                installed_at: Some("2026-08-03T12:00:00Z".into()),
                source: Some("upgrade".into()),
                source_url: None,
                sha256: None,
                variant: Some("linux-amd64".into()),
                os: Some("linux".into()),
                signature_ok: Some(true),
            },
        )
        .unwrap();
        let m = read_meta(root.path(), "1.0.0").unwrap().unwrap();
        assert_eq!(m.version, "1.0.0");
        assert_eq!(m.signature_ok, Some(true));
    }

    #[test]
    fn sanitize_rejects_empty_and_reserved() {
        assert!(sanitize_version_id("").is_err());
        assert!(sanitize_version_id("   ").is_err());
        assert!(sanitize_version_id("bad name").is_err());
        assert!(sanitize_version_id("a@b").is_err());
        assert!(sanitize_version_id("CON").is_err());
        assert!(sanitize_version_id("nul").is_err());
        assert!(sanitize_version_id("2.20.0-rc.1").is_ok());
        assert!(sanitize_version_id("2.20.0_build1").is_ok());
    }

    #[test]
    fn parse_version_handles_messy_output() {
        assert_eq!(
            parse_version_from_preflight_output("madmail-v2 2.20.1 (debug)\nextra\n"),
            "2.20.1"
        );
        assert_eq!(
            parse_version_from_preflight_output("\n\n  madmail-v2 3.0.0  \n"),
            "3.0.0"
        );
        // No digit token → sanitized first line or unknown-*
        let fallback = parse_version_from_preflight_output("only-text-here");
        assert!(
            fallback.starts_with("only-text-here") || fallback.starts_with("unknown-"),
            "got {fallback}"
        );
    }

    #[test]
    fn normalize_release_tag_strips_v() {
        assert_eq!(normalize_release_tag("v2.20.1"), "2.20.1");
        assert_eq!(normalize_release_tag("2.20.1"), "2.20.1");
        assert_eq!(normalize_release_tag("  v1.0.0  "), "1.0.0");
    }

    #[test]
    fn path_helpers_layout() {
        let root = PathBuf::from("/opt/madmail");
        assert_eq!(versions_dir(&root), PathBuf::from("/opt/madmail/versions"));
        assert_eq!(
            version_dir(&root, "2.1.0"),
            PathBuf::from("/opt/madmail/versions/2.1.0")
        );
        assert_eq!(
            version_binary_path(&root, "2.1.0"),
            PathBuf::from(format!(
                "/opt/madmail/versions/2.1.0/{}",
                binary_file_name()
            ))
        );
        assert_eq!(
            current_link_path(&root),
            PathBuf::from("/opt/madmail/current")
        );
        assert_eq!(default_keep(), 5);
        assert_eq!(
            github_releases_api_url(),
            "https://api.github.com/repos/themadorg/madmail/releases"
        );
        #[cfg(not(windows))]
        assert_eq!(binary_file_name(), "madmail");
        #[cfg(windows)]
        assert_eq!(binary_file_name(), "madmail.exe");
    }

    #[test]
    fn host_release_asset_name_is_os_specific() {
        let name = host_release_asset_name();
        assert!(name.starts_with("madmail-"));
        assert!(name.contains("amd64") || name.contains("arm64"));
        #[cfg(target_os = "linux")]
        assert!(name.contains("linux"));
        #[cfg(windows)]
        assert!(name.contains("windows"));
        // Must never claim /opt-style paths in the asset name
        assert!(!name.contains("/opt"));
    }

    #[test]
    fn list_installed_empty_without_tree() {
        let root = TempDir::new().unwrap();
        assert!(list_installed(root.path()).unwrap().is_empty());
        assert_eq!(resolve_active_version(root.path()).unwrap(), None);
    }

    #[test]
    fn list_skips_dirs_without_binary() {
        let root = TempDir::new().unwrap();
        ensure_install_layout(root.path()).unwrap();
        fs::create_dir_all(version_dir(root.path(), "2.0.0")).unwrap();
        // no binary → not listed
        assert!(list_installed(root.path()).unwrap().is_empty());
    }

    #[test]
    fn set_active_missing_version_errors() {
        let root = TempDir::new().unwrap();
        let stable = root.path().join("bin").join(binary_file_name());
        let err = set_active(root.path(), "9.9.9", &stable).unwrap_err();
        assert!(
            err.to_string().contains("not found") || err.to_string().contains("9.9.9"),
            "got: {err}"
        );
    }

    #[test]
    fn set_active_rejects_invalid_id() {
        let root = TempDir::new().unwrap();
        let stable = root.path().join("s");
        assert!(set_active(root.path(), "../evil", &stable).is_err());
    }

    #[test]
    fn switch_active_between_versions() {
        let root = TempDir::new().unwrap();
        let stable = root.path().join("bin").join(binary_file_name());
        let src = root.path().join("p");
        write_fake_bin(&src);
        for v in ["1.0.0", "2.0.0"] {
            install_candidate(
                root.path(),
                v,
                &src,
                VersionMeta {
                    version: v.into(),
                    installed_at: None,
                    source: Some("test".into()),
                    source_url: None,
                    sha256: None,
                    variant: None,
                    os: None,
                    signature_ok: Some(true),
                },
            )
            .unwrap();
        }
        set_active(root.path(), "1.0.0", &stable).unwrap();
        assert_eq!(
            resolve_active_version(root.path()).unwrap().as_deref(),
            Some("1.0.0")
        );
        set_active(root.path(), "2.0.0", &stable).unwrap();
        assert_eq!(
            resolve_active_version(root.path()).unwrap().as_deref(),
            Some("2.0.0")
        );
        // stable link should resolve into 2.0.0 tree
        let real = fs::canonicalize(&stable).unwrap();
        assert!(
            real.to_string_lossy().contains("2.0.0"),
            "stable points at {}",
            real.display()
        );
    }

    #[test]
    fn prune_keep_zero_removes_all_non_active() {
        let root = TempDir::new().unwrap();
        let stable = root.path().join("bin").join(binary_file_name());
        let src = root.path().join("p");
        write_fake_bin(&src);
        for v in ["1.0.0", "2.0.0", "3.0.0"] {
            install_candidate(
                root.path(),
                v,
                &src,
                VersionMeta {
                    version: v.into(),
                    installed_at: None,
                    source: None,
                    source_url: None,
                    sha256: None,
                    variant: None,
                    os: None,
                    signature_ok: None,
                },
            )
            .unwrap();
        }
        set_active(root.path(), "3.0.0", &stable).unwrap();
        let removed = prune(root.path(), 0).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(!removed.iter().any(|v| v == "3.0.0"));
        let left = list_installed(root.path()).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].version, "3.0.0");
        assert!(left[0].active);
    }

    #[test]
    fn prune_never_removes_only_active() {
        let root = TempDir::new().unwrap();
        let stable = root.path().join("bin").join(binary_file_name());
        let src = root.path().join("p");
        write_fake_bin(&src);
        install_candidate(
            root.path(),
            "5.0.0",
            &src,
            VersionMeta {
                version: "5.0.0".into(),
                installed_at: None,
                source: None,
                source_url: None,
                sha256: None,
                variant: None,
                os: None,
                signature_ok: None,
            },
        )
        .unwrap();
        set_active(root.path(), "5.0.0", &stable).unwrap();
        let removed = prune(root.path(), 0).unwrap();
        assert!(removed.is_empty());
        assert_eq!(list_installed(root.path()).unwrap().len(), 1);
    }

    #[test]
    fn remove_nonexistent_errors() {
        let root = TempDir::new().unwrap();
        let err = remove_version(root.path(), "0.0.1").unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn remove_non_active_ok() {
        let root = TempDir::new().unwrap();
        let stable = root.path().join("bin").join(binary_file_name());
        let src = root.path().join("p");
        write_fake_bin(&src);
        for v in ["1.0.0", "2.0.0"] {
            install_candidate(
                root.path(),
                v,
                &src,
                VersionMeta {
                    version: v.into(),
                    installed_at: None,
                    source: None,
                    source_url: None,
                    sha256: None,
                    variant: None,
                    os: None,
                    signature_ok: None,
                },
            )
            .unwrap();
        }
        set_active(root.path(), "2.0.0", &stable).unwrap();
        remove_version(root.path(), "1.0.0").unwrap();
        let left = list_installed(root.path()).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].version, "2.0.0");
    }

    #[test]
    fn install_candidate_overwrites_same_version() {
        let root = TempDir::new().unwrap();
        let src_a = root.path().join("a");
        let src_b = root.path().join("b");
        write_fake_bin(&src_a);
        {
            let mut f = File::create(&src_b).unwrap();
            f.write_all(b"#!/bin/sh\necho madmail-v2 9.9.9\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&src_b, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let meta = VersionMeta {
            version: "1.0.0".into(),
            installed_at: None,
            source: Some("first".into()),
            source_url: None,
            sha256: None,
            variant: None,
            os: None,
            signature_ok: Some(true),
        };
        install_candidate(root.path(), "1.0.0", &src_a, meta.clone()).unwrap();
        install_candidate(
            root.path(),
            "1.0.0",
            &src_b,
            VersionMeta {
                source: Some("second".into()),
                ..meta
            },
        )
        .unwrap();
        let bytes = fs::read(version_binary_path(root.path(), "1.0.0")).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("9.9.9"));
        assert_eq!(
            read_meta(root.path(), "1.0.0")
                .unwrap()
                .unwrap()
                .source
                .as_deref(),
            Some("second")
        );
    }

    #[test]
    fn merge_local_only_and_remote_only() {
        let local = vec![InstalledVersion {
            version: "0.9.0".into(),
            path: PathBuf::from("/l"),
            binary: PathBuf::from("/l/b"),
            active: false,
            meta: None,
        }];
        let remote = vec![RemoteRelease {
            version: "1.0.0".into(),
            published_at: None,
            asset: Some("x".into()),
            asset_size: Some(1),
            is_latest: true,
        }];
        let merged = merge_local_and_remote(&local, &remote);
        assert_eq!(merged.len(), 2);
        let local_only = merged.iter().find(|e| e.version == "0.9.0").unwrap();
        assert_eq!(local_only.source, "local");
        assert!(local_only.installed);
        assert!(!local_only.remote_latest);
        let remote_only = merged.iter().find(|e| e.version == "1.0.0").unwrap();
        assert_eq!(remote_only.source, "remote");
        assert!(!remote_only.installed);
        assert!(remote_only.remote_latest);
    }

    #[test]
    fn merge_empty_inputs() {
        assert!(merge_local_and_remote(&[], &[]).is_empty());
    }

    #[test]
    fn parse_github_skips_draft_and_prerelease() {
        let body = r#"[
          {"tag_name":"v3.0.0-rc1","draft":false,"prerelease":true,"assets":[]},
          {"tag_name":"v2.99.0","draft":true,"prerelease":false,"assets":[]},
          {"tag_name":"v2.20.0","draft":false,"prerelease":false,
           "assets":[{"name":"madmail-windows-amd64.tar.gz","size":50}]}
        ]"#;
        let rels = parse_github_releases_json(body, "windows").unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].version, "2.20.0");
        assert!(rels[0].is_latest);
        assert_eq!(
            rels[0].asset.as_deref(),
            Some("madmail-windows-amd64.tar.gz")
        );
    }

    #[test]
    fn parse_github_invalid_json_errors() {
        assert!(parse_github_releases_json("not-json", "linux").is_err());
        assert!(parse_github_releases_json("{}", "linux").is_err());
    }

    #[test]
    fn parse_github_caps_at_twenty() {
        let mut items = Vec::new();
        for i in 0..30 {
            items.push(format!(
                r#"{{"tag_name":"v1.0.{i}","draft":false,"prerelease":false,"assets":[]}}"#
            ));
        }
        let body = format!("[{}]", items.join(","));
        let rels = parse_github_releases_json(&body, "linux").unwrap();
        assert_eq!(rels.len(), 20);
        assert!(rels[0].is_latest);
        assert!(!rels[1].is_latest);
    }

    #[test]
    fn install_root_prefers_install_root_over_opt_alias() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();
        std::env::set_var(ENV_INSTALL_ROOT, a.path());
        std::env::set_var(ENV_OPT_ROOT_ALIAS, b.path());
        assert_eq!(install_root(), a.path());
        std::env::remove_var(ENV_INSTALL_ROOT);
        std::env::remove_var(ENV_OPT_ROOT_ALIAS);
    }

    #[cfg(not(windows))]
    #[test]
    fn install_root_opt_alias_when_install_root_unset() {
        std::env::remove_var(ENV_INSTALL_ROOT);
        let b = TempDir::new().unwrap();
        std::env::set_var(ENV_OPT_ROOT_ALIAS, b.path());
        assert_eq!(install_root(), b.path());
        std::env::remove_var(ENV_OPT_ROOT_ALIAS);
    }

    #[test]
    fn ensure_install_layout_creates_versions() {
        let root = TempDir::new().unwrap();
        ensure_install_layout(root.path()).unwrap();
        assert!(versions_dir(root.path()).is_dir());
    }

    #[test]
    fn read_meta_missing_is_none() {
        let root = TempDir::new().unwrap();
        assert!(read_meta(root.path(), "1.0.0").unwrap().is_none());
    }

    #[test]
    fn install_candidate_rejects_bad_version_id() {
        let root = TempDir::new().unwrap();
        let src = root.path().join("p");
        write_fake_bin(&src);
        let err = install_candidate(
            root.path(),
            "bad/id",
            &src,
            VersionMeta {
                version: "bad".into(),
                installed_at: None,
                source: None,
                source_url: None,
                sha256: None,
                variant: None,
                os: None,
                signature_ok: None,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("invalid"), "got: {err}");
    }

    #[test]
    fn windows_style_paths_never_use_opt_in_default_stable_on_windows_cfg() {
        // Cross-check: default_install_root documentation contract for non-Windows.
        #[cfg(not(windows))]
        {
            assert_eq!(
                default_stable_binary_path(),
                PathBuf::from("/usr/local/bin/madmail")
            );
        }
        #[cfg(windows)]
        {
            let p = default_stable_binary_path();
            let s = p.to_string_lossy();
            assert!(s.contains("Madmail") || s.contains("madmail"));
            assert!(!s.contains("/opt"));
            assert!(s.ends_with("madmail.exe") || s.contains("madmail.exe"));
        }
    }
}
