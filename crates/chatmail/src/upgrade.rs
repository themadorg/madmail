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

use chatmail_config::Args;
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

/// Archive member path is safe to consider (no absolute paths, `..`, etc.).
///
/// We never unpack the archive into a directory tree — only stream the chosen
/// member into a caller-owned temp file — but still reject traversal names so a
/// malicious archive cannot select a surprising member path.
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

fn tar_member_basename(name: &str) -> &str {
    Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(name)
}

/// True when the archive member is the release binary (`madmail`, any safe path).
///
/// Official packaging (`scripts/publish.sh`) always puts a single member named
/// `madmail` in `madmail-linux-*.tar.gz`. Nested names like `bin/madmail` are
/// also accepted.
fn is_madmail_member(name: &str) -> bool {
    tar_member_basename(name).eq_ignore_ascii_case("madmail")
}

/// Open a release `.tar.gz` / `.tgz` and extract the signed `madmail` binary to `dest`.
///
/// Safety properties:
/// - only regular file members are considered (no dirs/symlinks/hardlinks)
/// - member paths with `..` or absolute components are ignored
/// - only a member whose basename is `madmail` is extracted
/// - bytes are streamed into `dest` (never unpack the whole archive to disk)
/// - size is capped at [`MAX_DOWNLOAD_SIZE`]
///
/// The extracted file is the same object a local/`raw URL` upgrade would use;
/// callers must still run [`perform_upgrade`] (signature check, replace, …).
fn extract_binary_from_tar_gz(archive_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive_path).map_err(|e| {
        ChatmailError::config(format!(
            "failed to open archive {}: {e}",
            archive_path.display()
        ))
    })?;
    let mut archive = Archive::new(GzDecoder::new(file));

    // Pass 1: locate the `madmail` member (official release layout).
    let mut chosen: Option<(String, u64)> = None;
    let mut safe_file_count = 0u32;
    for entry in archive.entries().map_err(|e| {
        ChatmailError::config(format!(
            "failed to read archive (is this a valid .tar.gz?): {e}"
        ))
    })? {
        let entry = entry.map_err(|e| ChatmailError::config(format!("corrupt archive: {e}")))?;
        // Regular files only — never follow/extract symlinks or special nodes.
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
        safe_file_count += 1;
        if is_madmail_member(&name) {
            let size = entry.header().size().unwrap_or(0);
            chosen = Some((name, size));
            break;
        }
    }

    let (member, size) = match chosen {
        Some(c) => c,
        None if safe_file_count == 0 => {
            return Err(ChatmailError::config(
                "archive contains no extractable files (expected a signed madmail binary)",
            ));
        }
        None => {
            return Err(ChatmailError::config(
                "archive has no member named 'madmail' (official releases pack the signed \
                 binary as 'madmail' inside the .tar.gz)",
            ));
        }
    };

    if size > MAX_DOWNLOAD_SIZE {
        return Err(ChatmailError::config(format!(
            "archive member too large: {size} bytes (max {} MB)",
            MAX_DOWNLOAD_SIZE / (1024 * 1024)
        )));
    }

    eprintln!("📦 Extracting madmail binary from archive...");

    // Pass 2: re-open and stream only the chosen member into `dest`.
    // We deliberately do not call Archive::unpack — that would write member
    // paths to disk and is harder to make path-safe.
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
        eprintln!("✅ Extracted madmail binary ({n} bytes)");
        return Ok(());
    }

    Err(ChatmailError::config(format!(
        "archive member '{member}' not found during extraction"
    )))
}

/// Entry point for `chatmail upgrade` and `chatmail update` (Madmail `upgradeCommand`).
pub fn upgrade_command(input: &str, args: &Args) -> Result<()> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ChatmailError::config("PATH or URL is required"));
    }
    let result = if is_download_url(input) {
        handle_update_url(input, args)
    } else {
        perform_upgrade(Path::new(input), args)
    };
    if result.is_ok() && args.json {
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

/// Download from a URL, then run the **same** [`perform_upgrade`] path used for
/// local binaries.
///
/// - raw binary URL → temp file → `perform_upgrade` (signature, replace, …)
/// - `.tar.gz` / `.tgz` URL → temp archive → extract `madmail` → `perform_upgrade`
///
/// Local path upgrades never enter this function (`upgrade_command` calls
/// `perform_upgrade` directly).
fn handle_update_url(url: &str, args: &Args) -> Result<()> {
    check_supported_url_archive(url)?;

    // Unique suffix so concurrent upgrade attempts (and unit tests) do not collide.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let download_path = std::env::temp_dir().join(format!(
        "madmail-update-{}-{}",
        std::process::id(),
        unique
    ));
    let mut tmp_file = File::create(&download_path).map_err(|e| {
        ChatmailError::config(format!(
            "failed to create temp file {}: {e}",
            download_path.display()
        ))
    })?;

    let cleanup_download = || {
        let _ = fs::remove_file(&download_path);
    };

    eprintln!("📥 Downloading {url}...");

    let client = build_download_client()?;
    let resp = client.get(url).send().map_err(|e| {
        cleanup_download();
        ChatmailError::config(format!("failed to download: {e}"))
    })?;

    if !resp.status().is_success() {
        cleanup_download();
        return Err(ChatmailError::config(format!(
            "download failed with status: {}",
            resp.status()
        )));
    }

    if let Some(len) = resp.content_length() {
        if len > MAX_DOWNLOAD_SIZE {
            cleanup_download();
            return Err(ChatmailError::config(format!(
                "file too large: {len} bytes (max {} MB)",
                MAX_DOWNLOAD_SIZE / (1024 * 1024)
            )));
        }
    }

    let mut limited = resp.take(MAX_DOWNLOAD_SIZE + 1);
    let n = io::copy(&mut limited, &mut tmp_file).map_err(|e| {
        cleanup_download();
        ChatmailError::config(format!("failed to save download: {e}"))
    })?;
    drop(tmp_file);

    if n > MAX_DOWNLOAD_SIZE {
        cleanup_download();
        return Err(ChatmailError::config(format!(
            "download exceeded maximum size of {} MB, aborting",
            MAX_DOWNLOAD_SIZE / (1024 * 1024)
        )));
    }

    let n = fs::metadata(&download_path)
        .map_err(|e| {
            cleanup_download();
            ChatmailError::config(format!("temp file metadata: {e}"))
        })?
        .len();
    eprintln!("✅ Downloaded {n} bytes");

    // If the URL is a release archive, extract the signed `madmail` binary first.
    // Signature verification must never run on the .tar.gz bytes themselves.
    let (bin_path, extracted_tmp) = if is_tar_gz_url(url) {
        let extracted = std::env::temp_dir().join(format!(
            "madmail-update-bin-{}-{}",
            std::process::id(),
            unique
        ));
        if let Err(e) = extract_binary_from_tar_gz(&download_path, &extracted) {
            cleanup_download();
            let _ = fs::remove_file(&extracted);
            return Err(e);
        }
        cleanup_download(); // archive no longer needed
        (extracted, true)
    } else {
        (download_path.clone(), false)
    };

    // Traditional upgrade path (identical to local-path upgrades).
    let result = perform_upgrade(&bin_path, args);
    if extracted_tmp {
        let _ = fs::remove_file(&bin_path);
    } else {
        let _ = fs::remove_file(&download_path);
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
pub fn perform_upgrade(new_bin_path: &Path, args: &Args) -> Result<()> {
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

    // Run post-upgrade hooks from the *new* binary so first upgrades that ship
    // html-migrate still work (this process is still the old code).
    run_post_upgrade_www_migrate(&real_bin_path, args);

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

/// Ask the new binary to migrate custom `www_dir` Go templates (interactive).
fn run_post_upgrade_www_migrate(new_bin: &Path, args: &Args) {
    if !args.config.is_file() {
        eprintln!(
            "ℹ️ Config not found at {} — skipping custom www template check \
             (run: madmail --config <path> html-migrate)",
            args.config.display()
        );
        return;
    }

    // Never re-exec with --json: child would print a second JSON envelope on stdout
    // and break scripted `madmail update --json` parsers. Operators can migrate later.
    if args.json {
        eprintln!(
            "ℹ️ --json upgrade: not prompting for www template migration. \
             If you use a custom www_dir with Go templates, run: \
             madmail --config {} html-migrate",
            args.config.display()
        );
        return;
    }

    eprintln!("🌐 Checking custom www templates (Go → Minijinja)...");
    let mut cmd = Command::new(new_bin);
    cmd.arg("--config").arg(&args.config).arg("html-migrate");
    // Inherit stdin so interactive [y/N] works when the operator is at a TTY.
    cmd.stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    match cmd.status() {
        Ok(st) if st.success() => {}
        Ok(st) => {
            eprintln!(
                "⚠️ html-migrate exited with status {st} — you can re-run: \
                 madmail --config {} html-migrate",
                args.config.display()
            );
        }
        Err(e) => {
            eprintln!(
                "⚠️ Could not run html-migrate on the new binary ({e}). \
                 If you use a custom www_dir with Go templates, run: \
                 madmail --config {} html-migrate",
                args.config.display()
            );
        }
    }
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use tar::Header;

    fn test_args() -> Args {
        Args {
            config: PathBuf::from("/nonexistent/madmail.conf"),
            state_dir: PathBuf::from("/tmp"),
            boot_once: false,
            json: false,
        }
    }

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

    /// Minimal HTTP server: one GET returns `body` then exits.
    fn serve_once(body: Vec<u8>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
        });
        ready_rx.recv().unwrap();
        (format!("http://{addr}/madmail-linux-amd64.tar.gz"), handle)
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
    fn check_supported_url_archive_rejects_unsupported_formats() {
        for url in [
            "https://example.com/madmail.zip",
            "https://example.com/madmail.tar.bz2",
            "https://example.com/a.tar.xz?x=1",
            "https://example.com/bin.7z",
            "https://example.com/bin.rar",
            "https://example.com/plain.tar",
        ] {
            let err = check_supported_url_archive(url).unwrap_err();
            assert!(
                err.to_string().contains("unsupported archive format"),
                "url={url} got: {err}"
            );
        }
        assert!(check_supported_url_archive("https://example.com/madmail").is_ok());
        assert!(check_supported_url_archive("https://example.com/a.tar.gz").is_ok());
        assert!(check_supported_url_archive("https://example.com/a.tgz#frag").is_ok());
    }

    #[test]
    fn upgrade_command_requires_input() {
        let err = upgrade_command("  ", &test_args()).unwrap_err();
        assert!(err.to_string().contains("PATH or URL is required"));
    }

    #[test]
    fn upgrade_command_rejects_zip_url_without_download() {
        // Must fail before any network I/O.
        let err = upgrade_command("https://example.com/madmail.zip", &test_args()).unwrap_err();
        assert!(
            err.to_string().contains("unsupported archive format"),
            "got: {err}"
        );
    }

    #[test]
    fn upgrade_command_rejects_tar_bz2_url_without_download() {
        let err =
            upgrade_command("https://example.com/madmail.tar.bz2", &test_args()).unwrap_err();
        assert!(
            err.to_string().contains("unsupported archive format"),
            "got: {err}"
        );
    }

    #[test]
    fn extract_binary_requires_madmail_member() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("release.tar.gz");
        let dest = dir.path().join("out");
        let payload = b"signed-binary-bytes";
        // Official layout: other files may exist; only `madmail` is extracted.
        write_tar_gz(&archive, &[("README", b"hi"), ("madmail", payload)]);
        extract_binary_from_tar_gz(&archive, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), payload);
    }

    #[test]
    fn extract_binary_accepts_nested_madmail_basename() {
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
        assert!(is_madmail_member("madmail"));
        assert!(is_madmail_member("bin/madmail"));
        assert!(!is_madmail_member("madmail-linux-amd64"));
    }

    #[test]
    fn extract_binary_prefers_madmail_among_many_files() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("many.tar.gz");
        let dest = dir.path().join("out");
        let payload = b"safe-madmail";
        write_tar_gz(
            &archive,
            &[
                ("docs/README", b"x"),
                ("notes.txt", b"y"),
                ("madmail", payload),
                ("extra/bin", b"z"),
            ],
        );
        extract_binary_from_tar_gz(&archive, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), payload);
    }

    #[test]
    fn extract_binary_rejects_without_madmail_member() {
        let dir = tempfile::tempdir().unwrap();
        // Sole member with a different name is not enough — must be `madmail`.
        let sole = dir.path().join("sole.tgz");
        write_tar_gz(&sole, &[("madmail-linux-amd64", b"only-one")]);
        let err = extract_binary_from_tar_gz(&sole, &dir.path().join("out")).unwrap_err();
        assert!(
            err.to_string().contains("no member named 'madmail'"),
            "got: {err}"
        );

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
            .contains("no member named 'madmail'"));
    }

    /// Archive bytes themselves are not a signed binary — verification on the
    /// `.tar.gz` must fail; only the extracted `madmail` member is signed.
    #[test]
    fn signature_runs_on_extracted_binary_not_archive() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("release.tar.gz");
        let extracted = dir.path().join("madmail");
        // Payload large enough for a signature trailer; not actually signed.
        let payload = vec![b'P'; 200];
        write_tar_gz(&archive, &[("madmail", &payload)]);

        // Archive as a whole is not the signed object.
        assert!(
            !verify_signature(&archive).unwrap_or(false),
            "archive itself must not pass signature verification"
        );

        extract_binary_from_tar_gz(&archive, &extracted).unwrap();
        assert_eq!(fs::read(&extracted).unwrap(), payload);
        // Extracted payload is what perform_upgrade will verify (unsigned here).
        assert!(!verify_signature(&extracted).unwrap());
    }

    #[test]
    fn handle_update_url_extracts_tar_gz_then_signature_check() {
        // URL path: download → extract madmail → traditional verify.
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("release.tar.gz");
        let payload = vec![b'U'; 128];
        write_tar_gz(&archive, &[("madmail", &payload)]);
        let body = fs::read(&archive).unwrap();
        let (url, server) = serve_once(body);

        let err = upgrade_command(&url, &test_args()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("INVALID SIGNATURE"),
            "expected signature failure after extract, got: {msg}"
        );
        server.join().unwrap();
    }

    #[test]
    fn handle_update_url_raw_binary_skips_extract() {
        // Raw binary URL: no archive step; same traditional verify.
        let body = vec![b'R'; 128];
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
        });
        ready_rx.recv().unwrap();
        let url = format!("http://{addr}/madmail");

        let err = upgrade_command(&url, &test_args()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("INVALID SIGNATURE"),
            "expected signature failure on raw URL, got: {msg}"
        );
        server.join().unwrap();
    }

    #[test]
    fn local_path_upgrade_still_verifies_signature() {
        // Local binary path must not attempt archive extraction.
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("madmail-signed");
        fs::write(&bin, vec![b'L'; 128]).unwrap();
        let err = upgrade_command(bin.to_str().unwrap(), &test_args()).unwrap_err();
        assert!(
            err.to_string().contains("INVALID SIGNATURE"),
            "got: {err}"
        );
    }

    /// When the official signing key is available, prove signed `madmail` inside
    /// `.tar.gz` passes verification (the full traditional check) after extract.
    #[test]
    fn signed_madmail_inside_tar_gz_passes_verify_after_extract() {
        let Some(key_path) = official_private_key_path() else {
            eprintln!("skip: official private key not found");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("madmail");
        // Small fake binary body (not a real ELF); signature is what matters.
        fs::write(&payload, b"MADMAIL_TEST_PAYLOAD_FOR_SIGNATURE_CHECK_0123456789").unwrap();
        sign_with_official_key(&payload, &key_path);

        assert!(
            verify_signature(&payload).unwrap(),
            "signed payload must verify before packaging"
        );

        let archive = dir.path().join("madmail-linux-amd64.tar.gz");
        let bytes = fs::read(&payload).unwrap();
        write_tar_gz(&archive, &[("madmail", &bytes)]);

        // Archive itself is NOT the signed binary.
        assert!(!verify_signature(&archive).unwrap_or(false));

        let extracted = dir.path().join("extracted");
        extract_binary_from_tar_gz(&archive, &extracted).unwrap();
        assert!(
            verify_signature(&extracted).unwrap(),
            "extracted madmail must pass the traditional signature check"
        );

        // Full URL pipeline: download .tar.gz → extract → perform_upgrade verify.
        let body = fs::read(&archive).unwrap();
        let (url, server) = serve_once(body);
        let err = upgrade_command(&url, &test_args()).unwrap_err();
        let msg = err.to_string();
        // Signature OK; non-root should fail before replace (or root-only env).
        assert!(
            msg.contains("must be run as root") || msg.contains("Upgrade complete"),
            "expected post-signature traditional path, got: {msg}"
        );
        server.join().unwrap();
    }

    fn official_private_key_path() -> Option<PathBuf> {
        // Workspace is crates/chatmail → sibling `../imp` under the monorepo parent
        // is the release signing key that matches PUBLIC_KEY_HEX. Do not pick
        // `madmail/imp/private_key.hex` first — that may be a different local key.
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let preferred = manifest.join("../../../imp/private_key.hex");
        if preferred.is_file() {
            return Some(preferred);
        }
        None
    }

    fn sign_with_official_key(file: &Path, key_path: &Path) {
        let status = Command::new("python3")
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/publish/sign.py"),
            )
            .arg(file)
            .arg(key_path)
            .status()
            .expect("run sign.py");
        assert!(status.success(), "sign.py failed: {status}");
    }
}
