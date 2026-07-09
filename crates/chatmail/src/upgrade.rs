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

//! Signed binary upgrade.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use chatmail_types::{ChatmailError, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use tar::Archive;

/// Madmail release signing public key (`internal/auth/signature_key.go`).
const PUBLIC_KEY_HEX: &str = "7cb0bcc1d8e91e51f631c9ad6025e8e6e0222a27c3eeaf8608cf1c8430a6c6b0";

const SIGNATURE_LEN: usize = 64;
const MAX_DOWNLOAD_SIZE: u64 = 100 * 1024 * 1024; // 100 MB

fn verifying_key() -> Result<VerifyingKey> {
    let bytes = hex::decode(PUBLIC_KEY_HEX)
        .map_err(|e| ChatmailError::config(format!("invalid embedded public key: {e}")))?;
    VerifyingKey::from_bytes(
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| ChatmailError::config("public key must be 32 bytes"))?,
    )
    .map_err(|e| ChatmailError::config(format!("invalid public key: {e}")))
}

/// Verify Ed25519 signature appended as the last 64 bytes (Madmail `clitools.VerifySignature`).
pub fn verify_signature(path: &Path) -> Result<bool> {
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    if size < SIGNATURE_LEN as u64 {
        return Err(ChatmailError::config(
            "file too small to contain a signature",
        ));
    }
    let content_size = size - SIGNATURE_LEN as u64;

    let mut content = vec![0u8; content_size as usize];
    f.read_exact(&mut content)?;

    let mut sig_bytes = [0u8; SIGNATURE_LEN];
    f.read_exact(&mut sig_bytes)?;

    let sig = Signature::from_bytes(&sig_bytes);
    Ok(verifying_key()?.verify(&content, &sig).is_ok())
}

fn is_download_url(input: &str) -> bool {
    let s = input.trim();
    s.starts_with("http://") || s.starts_with("https://")
}

/// Path without `?query` / `#fragment` (for suffix checks on download URLs).
fn url_path(url: &str) -> &str {
    url.trim().split(['?', '#']).next().unwrap_or(url)
}

/// True when the URL points at a `.tar.gz` / `.tgz` release archive.
fn is_tar_gz_url(url: &str) -> bool {
    let path = url_path(url).to_ascii_lowercase();
    path.ends_with(".tar.gz") || path.ends_with(".tgz")
}

/// Reject archive formats other than `.tar.gz` / `.tgz` on download URLs.
fn check_supported_url_archive(url: &str) -> Result<()> {
    let path = url_path(url).to_ascii_lowercase();
    if path.ends_with(".tar.gz") || path.ends_with(".tgz") {
        return Ok(());
    }
    for ext in [".zip", ".tar.bz2", ".tar.xz", ".tar", ".7z", ".rar"] {
        if path.ends_with(ext) {
            return Err(ChatmailError::config(format!(
                "unsupported archive format '{ext}': only .tar.gz / .tgz archives are supported \
                 (or a raw signed binary URL)"
            )));
        }
    }
    Ok(())
}

fn is_safe_tar_member(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return false;
    }
    !path.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    })
}

/// Extract the signed binary from a release `.tar.gz` (usually a single `madmail` member).
fn extract_binary_from_tar_gz(archive_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive_path).map_err(|e| {
        ChatmailError::config(format!(
            "failed to open archive {}: {e}",
            archive_path.display()
        ))
    })?;
    let mut archive = Archive::new(GzDecoder::new(file));

    let mut chosen: Option<(String, u64)> = None;
    // First pass: prefer a member named `madmail`, else the sole regular file.
    let mut files: Vec<(String, u64)> = Vec::new();
    for entry in archive.entries().map_err(|e| {
        ChatmailError::config(format!(
            "failed to read archive (is this a valid .tar.gz?): {e}"
        ))
    })? {
        let entry = entry.map_err(|e| ChatmailError::config(format!("corrupt archive: {e}")))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let name = entry
            .path()
            .map_err(|e| ChatmailError::config(format!("invalid archive path: {e}")))?
            .to_string_lossy()
            .into_owned();
        if !is_safe_tar_member(&name) {
            continue;
        }
        let size = entry.header().size().unwrap_or(0);
        let base = Path::new(&name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&name);
        if base.eq_ignore_ascii_case("madmail") {
            chosen = Some((name, size));
            break;
        }
        files.push((name, size));
    }

    let (member, size) = if let Some(c) = chosen {
        c
    } else if files.len() == 1 {
        files.pop().unwrap()
    } else if files.is_empty() {
        return Err(ChatmailError::config(
            "archive contains no extractable files (expected a signed madmail binary)",
        ));
    } else {
        return Err(ChatmailError::config(
            "archive has multiple files and none named 'madmail'; use a release .tar.gz or a raw binary URL",
        ));
    };

    if size > MAX_DOWNLOAD_SIZE {
        return Err(ChatmailError::config(format!(
            "archive member too large: {size} bytes (max {} MB)",
            MAX_DOWNLOAD_SIZE / (1024 * 1024)
        )));
    }

    eprintln!("📦 Extracting binary from archive...");

    // Re-open and extract the chosen member.
    let file = File::open(archive_path).map_err(|e| {
        ChatmailError::config(format!(
            "failed to open archive {}: {e}",
            archive_path.display()
        ))
    })?;
    let mut archive = Archive::new(GzDecoder::new(file));
    for entry in archive
        .entries()
        .map_err(|e| ChatmailError::config(format!("failed to read archive: {e}")))?
    {
        let entry = entry.map_err(|e| ChatmailError::config(format!("corrupt archive: {e}")))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let name = entry
            .path()
            .map_err(|e| ChatmailError::config(format!("invalid archive path: {e}")))?
            .to_string_lossy()
            .into_owned();
        if name != member {
            continue;
        }

        let mut out = File::create(dest).map_err(|e| {
            ChatmailError::config(format!(
                "failed to create extracted binary {}: {e}",
                dest.display()
            ))
        })?;
        let mut limited = entry.take(MAX_DOWNLOAD_SIZE + 1);
        let n = io::copy(&mut limited, &mut out)
            .map_err(|e| ChatmailError::config(format!("failed to extract archive member: {e}")))?;
        out.flush().ok();
        out.sync_all().ok();
        if n > MAX_DOWNLOAD_SIZE {
            let _ = fs::remove_file(dest);
            return Err(ChatmailError::config(format!(
                "extracted binary exceeded maximum size of {} MB, aborting",
                MAX_DOWNLOAD_SIZE / (1024 * 1024)
            )));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dest, fs::Permissions::from_mode(0o755))?;
        }
        eprintln!("✅ Extracted {n} bytes");
        return Ok(());
    }

    Err(ChatmailError::config(format!(
        "archive member '{member}' not found during extraction"
    )))
}

/// Entry point for `chatmail upgrade` and `chatmail update` (Madmail `upgradeCommand`).
pub fn upgrade_command(input: &str, json: bool) -> Result<()> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ChatmailError::config("PATH or URL is required"));
    }
    let result = if is_download_url(input) {
        handle_update_url(input)
    } else {
        perform_upgrade(Path::new(input))
    };
    if result.is_ok() && json {
        let envelope = serde_json::json!({
            "ok": true,
            "command": "upgrade",
            "data": {}
        });
        if let Ok(body) = serde_json::to_string(&envelope) {
            println!("{body}");
        }
    }
    result
}

fn build_download_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(300))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| ChatmailError::config(format!("HTTP client: {e}")))
}

/// Download signed binary (or `.tar.gz`) to a temp file, then run `perform_upgrade`.
fn handle_update_url(url: &str) -> Result<()> {
    check_supported_url_archive(url)?;

    let tmp_path = std::env::temp_dir().join(format!("madmail-update-{}", std::process::id()));
    let mut tmp_file = File::create(&tmp_path).map_err(|e| {
        ChatmailError::config(format!(
            "failed to create temp file {}: {e}",
            tmp_path.display()
        ))
    })?;

    let cleanup = || {
        let _ = fs::remove_file(&tmp_path);
    };

    eprintln!("📥 Downloading {url}...");

    let client = build_download_client()?;
    let resp = client.get(url).send().map_err(|e| {
        cleanup();
        ChatmailError::config(format!("failed to download: {e}"))
    })?;

    if !resp.status().is_success() {
        cleanup();
        return Err(ChatmailError::config(format!(
            "download failed with status: {}",
            resp.status()
        )));
    }

    if let Some(len) = resp.content_length() {
        if len > MAX_DOWNLOAD_SIZE {
            cleanup();
            return Err(ChatmailError::config(format!(
                "file too large: {len} bytes (max {} MB)",
                MAX_DOWNLOAD_SIZE / (1024 * 1024)
            )));
        }
    }

    let mut limited = resp.take(MAX_DOWNLOAD_SIZE + 1);
    let n = io::copy(&mut limited, &mut tmp_file).map_err(|e| {
        cleanup();
        ChatmailError::config(format!("failed to save download: {e}"))
    })?;
    drop(tmp_file);

    if n > MAX_DOWNLOAD_SIZE {
        cleanup();
        return Err(ChatmailError::config(format!(
            "download exceeded maximum size of {} MB, aborting",
            MAX_DOWNLOAD_SIZE / (1024 * 1024)
        )));
    }

    let n = fs::metadata(&tmp_path)
        .map_err(|e| {
            cleanup();
            ChatmailError::config(format!("temp file metadata: {e}"))
        })?
        .len();
    eprintln!("✅ Downloaded {n} bytes");

    // Release artifacts may be raw signed binaries or `.tar.gz` archives containing one.
    let (bin_path, extracted_tmp) = if is_tar_gz_url(url) {
        let extracted =
            std::env::temp_dir().join(format!("madmail-update-bin-{}", std::process::id()));
        if let Err(e) = extract_binary_from_tar_gz(&tmp_path, &extracted) {
            cleanup();
            let _ = fs::remove_file(&extracted);
            return Err(e);
        }
        cleanup(); // archive no longer needed
        (extracted, true)
    } else {
        (tmp_path.clone(), false)
    };

    let result = perform_upgrade(&bin_path);
    if extracted_tmp {
        let _ = fs::remove_file(&bin_path);
    } else {
        cleanup();
    }
    result
}

fn systemd_service_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .map(|name| format!("{name}.service"))
        .unwrap_or_else(|| "madmail.service".into())
}

fn run_systemctl(args: &[&str]) {
    let _ = Command::new("systemctl").args(args).status();
}

/// Upgrade in place: verify signature, stop service, replace executable, start service.
pub fn perform_upgrade(new_bin_path: &Path) -> Result<()> {
    eprintln!("🔍 Verifying digital signature...");
    match verify_signature(new_bin_path)? {
        true => eprintln!("✅ Signature verification successful."),
        false => {
            return Err(ChatmailError::config(
                "INVALID SIGNATURE: this binary cannot be trusted; upgrade aborted",
            ));
        }
    }

    let current_bin = std::env::current_exe()
        .map_err(|e| ChatmailError::config(format!("failed to get current executable: {e}")))?;
    let real_bin_path = fs::canonicalize(&current_bin).unwrap_or(current_bin);

    eprintln!("🚀 Target binary: {}", real_bin_path.display());

    #[cfg(unix)]
    if unsafe { libc::geteuid() } != 0 {
        return Err(ChatmailError::config(
            "upgrade must be run as root (sudo) to manage services and replace the binary",
        ));
    }

    let service = systemd_service_name();
    eprintln!("⏹️ Stopping services...");
    run_systemctl(&["stop", &service]);
    run_systemctl(&["stop", "iroh-relay.service"]);
    thread::sleep(Duration::from_secs(1));

    eprintln!("🔄 Replacing binary...");
    let tmp_dir = real_bin_path
        .parent()
        .ok_or_else(|| ChatmailError::config("executable has no parent directory"))?;

    let tmp_path = tmp_dir.join(format!(".chatmail-upgrade-{}", std::process::id()));

    {
        let mut src = File::open(new_bin_path)?;
        let mut dst = File::create(&tmp_path)?;
        io::copy(&mut src, &mut dst)?;
        dst.sync_all()?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))?;
    }

    fs::rename(&tmp_path, &real_bin_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        ChatmailError::config(format!("failed to replace binary: {e}"))
    })?;

    eprintln!("▶️ Starting services...");
    if !Command::new("systemctl")
        .args(["start", &service])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        eprintln!("⚠️ Warning: failed to start {service}; try: systemctl start {service}");
    }

    let iroh_unit = PathBuf::from("/etc/systemd/system/iroh-relay.service");
    if iroh_unit.is_file()
        && !Command::new("systemctl")
            .args(["start", "iroh-relay.service"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    {
        eprintln!(
                "⚠️ Warning: failed to start iroh-relay.service; try: systemctl start iroh-relay.service"
            );
    }

    refresh_cli_docs_after_upgrade();

    eprintln!("🎉 Upgrade complete.");
    Ok(())
}

/// Rewrite man page and shell tab-completion scripts after the binary is replaced.
fn refresh_cli_docs_after_upgrade() {
    let name = crate::ctl::argv_binary_name();
    eprintln!("📚 Refreshing man page and shell completions for {name}...");
    match crate::ctl::install_cli_docs(&name, false) {
        Ok(()) => eprintln!("✅ Man page and shell completions updated."),
        Err(e) => eprintln!(
            "⚠️ Could not refresh man page/completions (tab completion may be stale until \
             `madmail completion bash | sudo tee /usr/share/bash-completion/completions/{name}`): {e}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tar::Header;

    fn write_tar_gz(path: &Path, members: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let enc = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(enc);
        for (name, data) in members {
            let mut header = Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, *name, *data).unwrap();
        }
        builder.finish().unwrap();
    }

    #[test]
    fn is_download_url_detects_http_and_https() {
        assert!(is_download_url("https://example.com/madmail"));
        assert!(is_download_url("http://127.0.0.1:8080/bin"));
        assert!(!is_download_url("/tmp/madmail-signed"));
        assert!(!is_download_url("./madmail"));
    }

    #[test]
    fn is_tar_gz_url_detects_suffixes() {
        assert!(is_tar_gz_url(
            "https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-amd64.tar.gz"
        ));
        assert!(is_tar_gz_url("https://example.com/a.tgz?token=x"));
        assert!(!is_tar_gz_url("https://example.com/madmail"));
        assert!(!is_tar_gz_url("https://example.com/madmail.tar.gz.asc"));
    }

    #[test]
    fn check_supported_url_archive_rejects_zip() {
        let err = check_supported_url_archive("https://example.com/madmail.zip").unwrap_err();
        assert!(err.to_string().contains("unsupported archive format"));
        assert!(check_supported_url_archive("https://example.com/madmail").is_ok());
        assert!(check_supported_url_archive("https://example.com/a.tar.gz").is_ok());
    }

    #[test]
    fn upgrade_command_requires_input() {
        let err = upgrade_command("  ", false).unwrap_err();
        assert!(err.to_string().contains("PATH or URL is required"));
    }

    #[test]
    fn upgrade_command_rejects_zip_url_without_download() {
        // Must fail before any network I/O.
        let err = upgrade_command("https://example.com/madmail.zip", false).unwrap_err();
        assert!(
            err.to_string().contains("unsupported archive format"),
            "got: {err}"
        );
    }

    #[test]
    fn extract_binary_prefers_madmail_member() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("release.tar.gz");
        let dest = dir.path().join("out");
        let payload = b"signed-binary-bytes";
        write_tar_gz(&archive, &[("README", b"hi"), ("madmail", payload)]);
        extract_binary_from_tar_gz(&archive, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), payload);
    }

    #[test]
    fn extract_binary_prefers_nested_madmail_basename() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("nested.tar.gz");
        let dest = dir.path().join("out");
        let payload = b"nested-madmail";
        write_tar_gz(&archive, &[("docs/README", b"x"), ("bin/madmail", payload)]);
        extract_binary_from_tar_gz(&archive, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), payload);
    }

    #[test]
    fn is_safe_tar_member_blocks_traversal() {
        assert!(is_safe_tar_member("madmail"));
        assert!(is_safe_tar_member("bin/madmail"));
        assert!(!is_safe_tar_member("../evil"));
        assert!(!is_safe_tar_member("/abs/path"));
        assert!(!is_safe_tar_member(""));
    }

    #[test]
    fn extract_binary_accepts_sole_member() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("sole.tgz");
        let dest = dir.path().join("out");
        write_tar_gz(&archive, &[("madmail-linux-amd64", b"only-one")]);
        extract_binary_from_tar_gz(&archive, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"only-one");
    }

    #[test]
    fn extract_binary_rejects_empty_and_ambiguous() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty.tar.gz");
        write_tar_gz(&empty, &[]);
        assert!(extract_binary_from_tar_gz(&empty, &dir.path().join("x"))
            .unwrap_err()
            .to_string()
            .contains("no extractable files"));

        let multi = dir.path().join("multi.tar.gz");
        write_tar_gz(&multi, &[("a", b"1"), ("b", b"2")]);
        assert!(extract_binary_from_tar_gz(&multi, &dir.path().join("y"))
            .unwrap_err()
            .to_string()
            .contains("multiple files"));
    }
}
