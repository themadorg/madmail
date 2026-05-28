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
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use reqwest::blocking::Client;
use chatmail_types::{ChatmailError, Result};

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

/// Entry point for `chatmail upgrade` and `chatmail update` (Madmail `upgradeCommand`).
pub fn upgrade_command(input: &str) -> Result<()> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ChatmailError::config("PATH or URL is required"));
    }
    if is_download_url(input) {
        handle_update_url(input)
    } else {
        perform_upgrade(Path::new(input))
    }
}

fn build_download_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(300))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| ChatmailError::config(format!("HTTP client: {e}")))
}

/// Download signed binary to a temp file, then run `perform_upgrade` (Madmail `handleUpdateURL`).
fn handle_update_url(url: &str) -> Result<()> {
    let tmp_path = std::env::temp_dir().join(format!("madmail-update-{}", std::process::id()));
    let mut tmp_file = File::create(&tmp_path).map_err(|e| {
        ChatmailError::config(format!("failed to create temp file {}: {e}", tmp_path.display()))
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

    let result = perform_upgrade(&tmp_path);
    cleanup();
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
    if iroh_unit.is_file() {
        if !Command::new("systemctl")
            .args(["start", "iroh-relay.service"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            eprintln!(
                "⚠️ Warning: failed to start iroh-relay.service; try: systemctl start iroh-relay.service"
            );
        }
    }

    eprintln!("🎉 Upgrade complete.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_download_url_detects_http_and_https() {
        assert!(is_download_url("https://example.com/madmail"));
        assert!(is_download_url("http://127.0.0.1:8080/bin"));
        assert!(!is_download_url("/tmp/madmail-signed"));
        assert!(!is_download_url("./madmail"));
    }

    #[test]
    fn upgrade_command_requires_input() {
        let err = upgrade_command("  ").unwrap_err();
        assert!(err.to_string().contains("PATH or URL is required"));
    }
}
