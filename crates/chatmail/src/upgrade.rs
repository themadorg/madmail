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

use std::fs::{self, File, OpenOptions};
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

/// Archive member path is safe to consider (no absolute paths, `..`, NUL, etc.).
///
/// We never unpack the archive into a directory tree — only stream the chosen
/// member into a caller-owned temp file — but still reject traversal names so a
/// malicious archive cannot select a surprising member path.
fn is_safe_tar_member(name: &str) -> bool {
    if name.is_empty() || name.contains('\0') {
        return false;
    }
    // Reject backslash paths that some unpackers treat as directory separators.
    if name.contains('\\') {
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

fn private_temp_path(prefix: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}-{}-{}", std::process::id(), unique))
}

/// Create a new private temp file (`O_CREAT|O_EXCL`, mode `0600` on Unix).
///
/// Avoids classic `/tmp` races: no follow/replace of a pre-planted symlink, and
/// contents are not world-readable while the signed binary sits on disk.
fn create_private_temp_file(prefix: &str) -> Result<(PathBuf, File)> {
    // Retry a few times if the rare name collision hits `create_new`.
    for _ in 0..8 {
        let path = private_temp_path(prefix);
        let mut opts = OpenOptions::new();
        opts.write(true).read(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        match opts.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(ChatmailError::config(format!(
                    "failed to create private temp file {}: {e}",
                    path.display()
                )));
            }
        }
    }
    Err(ChatmailError::config(
        "failed to create private temp file: too many name collisions",
    ))
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

        // `create_new` → O_EXCL: do not follow a pre-planted symlink at `dest`.
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Owner-only until after signature verification in perform_upgrade.
            opts.mode(0o600);
        }
        let mut out = opts.open(dest).map_err(|e| {
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
        // Executable bit only for the owner until root install replaces the binary;
        // perform_upgrade re-sets 0o755 on the installed path.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dest, fs::Permissions::from_mode(0o700))?;
        }
        eprintln!("✅ Extracted madmail binary ({n} bytes)");
        return Ok(());
    }

    Err(ChatmailError::config(format!(
        "archive member '{member}' not found during extraction"
    )))
}

/// Entry point for `chatmail upgrade` and `chatmail update` (Madmail `upgradeCommand`).
///
/// `accept_unsafe_https` maps to `--accept-unsafe-https`: allow HTTPS with untrusted TLS certs.
/// Without it, TLS is verified; on certificate failure an interactive TTY may prompt.
pub fn upgrade_command(input: &str, args: &Args, accept_unsafe_https: bool) -> Result<()> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ChatmailError::config(
            "PATH, URL, or the keyword `latest` is required",
        ));
    }
    // `update latest` / `upgrade latest` → GitHub Releases for this host (same security as URL).
    let resolved_latest;
    let input = if input.eq_ignore_ascii_case("latest") {
        resolved_latest = crate::version_manager::github_latest_asset_url();
        eprintln!("📦 Resolving GitHub latest: {resolved_latest}");
        resolved_latest.as_str()
    } else {
        input
    };
    let result = if is_download_url(input) {
        handle_update_url(input, args, accept_unsafe_https)
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

fn build_download_client(accept_invalid_certs: bool) -> Result<Client> {
    let mut builder = Client::builder().timeout(Duration::from_secs(300));
    if accept_invalid_certs {
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder
        .build()
        .map_err(|e| ChatmailError::config(format!("HTTP client: {e}")))
}

/// True when a reqwest error is (or wraps) a TLS/certificate failure.
fn is_tls_certificate_error(err: &reqwest::Error) -> bool {
    let mut blob = String::new();
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = cur {
        blob.push(' ');
        blob.push_str(&e.to_string());
        cur = e.source();
    }
    tls_error_blob_matches(&blob)
}

fn tls_error_blob_matches(blob: &str) -> bool {
    let s = blob.to_ascii_lowercase();
    s.contains("certificate")
        || s.contains("unknown issuer")
        || s.contains("not valid for name")
        || s.contains("invalid peer certificate")
        || (s.contains("cert")
            && (s.contains("tls") || s.contains("ssl") || s.contains("handshake")))
        || s.contains("pkix")
        || s.contains("self-signed")
        || s.contains("self signed")
}

/// Decide whether to disable TLS certificate verification for this download.
///
/// - `--accept-unsafe-https` → yes
/// - interactive TTY (and not `--json`) → prompt `[y/N]`
/// - otherwise → error (tell operator to pass `--accept-unsafe-https`)
fn allow_unsafe_tls(accept_unsafe_https: bool, args: &Args) -> Result<bool> {
    if accept_unsafe_https {
        return Ok(true);
    }
    use std::io::IsTerminal;
    if args.json || !std::io::stdin().is_terminal() {
        return Err(ChatmailError::config(
            "TLS certificate verification failed. Re-run with --accept-unsafe-https to allow \
             self-signed or untrusted certificates (Ed25519 signature verification of the \
             binary still applies after download).",
        ));
    }
    eprintln!("⚠️ TLS certificate verification failed for this download URL.");
    eprintln!(
        "   If you continue, HTTPS will not authenticate the server (self-signed/untrusted cert). \
         This never skips Ed25519 signature checks — unsigned or bad-signed binaries are still rejected."
    );
    let ok = crate::ctl::util::confirm("Accept unsafe HTTPS (TLS only) and continue?", false)?;
    if !ok {
        return Err(ChatmailError::config(
            "upgrade aborted: unsafe TLS not accepted",
        ));
    }
    Ok(true)
}

/// GET `url` with TLS certificate verification by default.
///
/// - `--accept-unsafe-https` → skip verification for this download immediately
/// - certificate error + interactive yes → retry without verification
/// - certificate error + non-interactive → error mentioning `--accept-unsafe-https`
fn download_url_response(
    url: &str,
    args: &Args,
    accept_unsafe_https: bool,
) -> Result<reqwest::blocking::Response> {
    if accept_unsafe_https {
        eprintln!(
            "⚠️ Downloading with TLS certificate verification disabled (--accept-unsafe-https). \
             Binary Ed25519 signature verification still applies."
        );
        let client = build_download_client(true)?;
        return client
            .get(url)
            .send()
            .map_err(|e| ChatmailError::config(format!("failed to download (unsafe TLS): {e}")));
    }

    let client = build_download_client(false)?;
    match client.get(url).send() {
        Ok(resp) => Ok(resp),
        Err(e) if is_tls_certificate_error(&e) => {
            // Interactive prompt (or hard error when non-interactive / --json).
            let _ = allow_unsafe_tls(false, args)?;
            eprintln!("⚠️ Proceeding without TLS certificate verification (operator confirmed).");
            let client = build_download_client(true)?;
            client.get(url).send().map_err(|e2| {
                ChatmailError::config(format!("failed to download (unsafe TLS): {e2}"))
            })
        }
        Err(e) => Err(ChatmailError::config(format!("failed to download: {e}"))),
    }
}

/// Download from a URL, then run the **same** [`perform_upgrade`] path used for
/// local binaries.
///
/// - raw binary URL → temp file → `perform_upgrade` (signature, replace, …)
/// - `.tar.gz` / `.tgz` URL → temp archive → extract `madmail` → `perform_upgrade`
///
/// Local path upgrades never enter this function (`upgrade_command` calls
/// `perform_upgrade` directly).
fn handle_update_url(url: &str, args: &Args, accept_unsafe_https: bool) -> Result<()> {
    check_supported_url_archive(url)?;
    warn_if_default_linux_asset(url);

    let (download_path, mut tmp_file) = create_private_temp_file("madmail-update")?;

    let cleanup_download = || {
        let _ = fs::remove_file(&download_path);
    };

    eprintln!("📥 Downloading {url}...");

    let resp = match download_url_response(url, args, accept_unsafe_https) {
        Ok(r) => r,
        Err(e) => {
            cleanup_download();
            return Err(e);
        }
    };

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
        // Unique path; extract opens with create_new (O_EXCL) + mode 0600.
        let extracted = private_temp_path("madmail-update-bin");
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

fn systemctl_succeeded(args: &[&str]) -> bool {
    Command::new("systemctl")
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Sibling backup path for the live binary (`/usr/local/bin/madmail` → `…/madmail.prev`).
fn backup_path_for(current: &Path) -> PathBuf {
    let mut name = current.as_os_str().to_owned();
    name.push(".prev");
    PathBuf::from(name)
}

/// Hint shown when a binary fails to run (wrong release variant / older glibc).
const VARIANT_HINT: &str = "\
If you see GLIBC_*. not found (or the loader refuses to start the binary), this host needs a \
different release asset than the default glibc build:\n\
  • madmail-linux-amd64-legacy.tar.gz  — older distros (e.g. Ubuntu 22.04)\n\
  • madmail-linux-amd64-musl.tar.gz    — static-ish musl alternative\n\
  • …-arm64-legacy / …-arm64-musl     — same for arm64\n\
Download from https://github.com/themadorg/madmail/releases and re-run update with that URL.";

/// Soft warning when the download URL looks like a default (non-legacy, non-musl) Linux asset.
fn warn_if_default_linux_asset(url: &str) {
    let path = url_path(url).to_ascii_lowercase();
    if !path.contains("madmail-linux-") {
        return;
    }
    if path.contains("-legacy") || path.contains("-musl") {
        return;
    }
    if !(path.contains("amd64") || path.contains("arm64") || path.contains("aarch64")) {
        return;
    }
    eprintln!(
        "ℹ️ Default Linux build selected. Hosts with older system glibc need a *-legacy \
         (or *-musl) asset from GitHub Releases. A host preflight runs before the live \
         binary is replaced; an incompatible build aborts the upgrade safely."
    );
}

/// Where the binary lives for a smoke/`version` check.
///
/// Staging extracts may stay owner-only. The **installed** path is executed by
/// systemd as `User=madmail` (not root), so it must be world-executable (`0755`).
/// Applying `0700` to the install path causes `Permission denied` / 203/EXEC
/// (GitHub issue #131).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinaryExecLocation {
    /// Download/extract temp or other non-service path.
    Staging,
    /// Live install path (e.g. `/usr/local/bin/madmail`).
    Installed,
}

#[cfg(unix)]
const STAGING_EXEC_MODE: u32 = 0o700;
#[cfg(unix)]
const INSTALLED_EXEC_MODE: u32 = 0o755;

/// Ensure the binary can actually execute on this host (`madmail version`).
///
/// Catches wrong-variant installs (default glibc build on older distros) **before**
/// services are stopped or the live binary is replaced. Signature verification must
/// already have passed — we only exec trusted, signed bytes.
/// Host preflight for live archives under the version tree (`versions use`).
///
/// Uses installed exec mode (`0755` on Unix) so a successful switch does not leave
/// `versions/<id>/madmail` owner-only (`0700`). Staging downloads still use staging
/// mode (`0700`) only on the upgrade download path.
pub fn preflight_binary_for_version_manager(new_bin: &Path) -> Result<()> {
    preflight_new_binary(new_bin, BinaryExecLocation::Installed)
}

fn capture_version_id(new_bin: &Path) -> String {
    match Command::new(new_bin).arg("version").output() {
        Ok(o) if o.status.success() => crate::version_manager::parse_version_from_preflight_output(
            &String::from_utf8_lossy(&o.stdout),
        ),
        _ => crate::version_manager::parse_version_from_preflight_output(env!("CARGO_PKG_VERSION")),
    }
}

fn install_into_version_tree(src: &Path, version_id: &str, root: &Path) -> Result<()> {
    use crate::version_manager::{install_candidate, VersionMeta};
    let meta = VersionMeta {
        version: version_id.to_string(),
        installed_at: Some(chrono_like_now()),
        source: Some("upgrade".into()),
        source_url: None,
        sha256: None,
        variant: None,
        os: Some(std::env::consts::OS.to_string()),
        signature_ok: Some(true),
    };
    install_candidate(root, version_id, src, meta)?;
    Ok(())
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

fn preflight_new_binary(new_bin: &Path, location: BinaryExecLocation) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = match location {
            BinaryExecLocation::Staging => STAGING_EXEC_MODE,
            // Service user (madmail) must be able to exec; root-only 0700 breaks systemd.
            BinaryExecLocation::Installed => INSTALLED_EXEC_MODE,
        };
        fs::set_permissions(new_bin, fs::Permissions::from_mode(mode)).map_err(|e| {
            ChatmailError::config(format!(
                "failed to set executable bit on {}: {e}",
                new_bin.display()
            ))
        })?;
    }

    let label = match location {
        BinaryExecLocation::Staging => "Preflight",
        BinaryExecLocation::Installed => "Smoke check",
    };
    eprintln!("🧪 {label}: running binary (`version`) on this host...");

    let output = match Command::new(new_bin).arg("version").output() {
        Ok(o) => o,
        Err(e) => {
            let not_replaced = matches!(location, BinaryExecLocation::Staging);
            return Err(ChatmailError::config(format!(
                "binary failed to execute (loader/ABI incompatibility?): {e}\n\
                 {}\n{VARIANT_HINT}",
                if not_replaced {
                    "The live binary was NOT replaced."
                } else {
                    "The installed binary cannot run."
                }
            )));
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            format!("exit status {}", output.status)
        };
        let not_replaced = matches!(location, BinaryExecLocation::Staging);
        return Err(ChatmailError::config(format!(
            "binary failed host check (`madmail version`):\n{detail}\n\n\
             {}\n{VARIANT_HINT}",
            if not_replaced {
                "The live binary was NOT replaced."
            } else {
                "The installed binary cannot run."
            }
        )));
    }

    let ver = String::from_utf8_lossy(&output.stdout);
    let first = ver
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("ok");
    eprintln!("✅ {label} OK ({first})");
    Ok(())
}

/// Copy the live binary to `*.prev` so a failed upgrade can be rolled back.
fn backup_current_binary(current: &Path) -> Result<PathBuf> {
    let backup = backup_path_for(current);
    eprintln!("💾 Backing up current binary to {}...", backup.display());
    fs::copy(current, &backup).map_err(|e| {
        ChatmailError::config(format!(
            "failed to back up current binary to {}: {e}",
            backup.display()
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Prefer the live mode; fall back to 0755 so the backup stays runnable.
        let mode = fs::metadata(current)
            .ok()
            .map(|m| m.permissions().mode())
            .unwrap_or(0o755);
        let _ = fs::set_permissions(&backup, fs::Permissions::from_mode(mode));
    }
    Ok(backup)
}

/// Restore `backup` over `current` (used when the new binary fails smoke/start).
fn restore_backup(backup: &Path, current: &Path) -> Result<()> {
    eprintln!(
        "↩️ Restoring previous binary from {} → {}...",
        backup.display(),
        current.display()
    );
    // Write via a temp sibling then rename so a partial copy cannot leave a
    // half-written executable at the install path.
    let parent = current
        .parent()
        .ok_or_else(|| ChatmailError::config("executable has no parent directory"))?;
    let tmp = parent.join(format!(".chatmail-rollback-{}", std::process::id()));
    fs::copy(backup, &tmp).map_err(|e| {
        ChatmailError::config(format!(
            "failed to stage rollback binary from {}: {e}",
            backup.display()
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(INSTALLED_EXEC_MODE))?;
    }
    fs::rename(&tmp, current).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        ChatmailError::config(format!(
            "failed to restore previous binary to {}: {e}",
            current.display()
        ))
    })?;
    Ok(())
}

/// After install: if the new binary cannot run, restore `backup` and restart services.
fn rollback_on_broken_install(
    real_bin_path: &Path,
    backup: &Path,
    service: &str,
    reason: &str,
) -> Result<()> {
    eprintln!("⚠️ {reason}");
    restore_backup(backup, real_bin_path)?;
    eprintln!("▶️ Restarting services with the restored binary...");
    if !systemctl_succeeded(&["start", service]) {
        eprintln!(
            "⚠️ Warning: failed to start {service} after rollback; try: systemctl start {service}"
        );
    }
    let iroh_unit = PathBuf::from("/etc/systemd/system/iroh-relay.service");
    if iroh_unit.is_file() && !systemctl_succeeded(&["start", "iroh-relay.service"]) {
        eprintln!(
            "⚠️ Warning: failed to start iroh-relay.service after rollback; try: systemctl start iroh-relay.service"
        );
    }
    Err(ChatmailError::config(format!(
        "upgrade rolled back: {reason}\n\
         Previous binary restored from {}.\n{VARIANT_HINT}",
        backup.display()
    )))
}

/// Upgrade: verify signature, preflight, install, activate, start.
///
/// Prefer the version tree (TDD 24): `install_candidate` into
/// `{install_root}/versions/<id>/` then flip `current` + stable PATH symlink.
/// Never in-place overwrite a file under the version tree (that would clobber
/// archived history after the first upgrade when PATH is a symlink into
/// `versions/<old>/`).
///
/// If the install root is not writable, fall back to legacy single-file replace
/// of a non-version-tree path (with `*.prev` backup).
///
/// Safety (issue #114):
/// - Host preflight (`new_bin version`) runs **before** services stop / activate.
/// - Versioned path: previous version remains on disk; rollback flips the pointer.
/// - Legacy path: live binary is copied to `*.prev` before replace.
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

    // Abort early (services still up, live binary untouched) if this host cannot run it.
    preflight_new_binary(new_bin_path, BinaryExecLocation::Staging)?;

    // Capture version id for the version tree (TDD 24).
    let version_id = capture_version_id(new_bin_path);

    #[cfg(unix)]
    if unsafe { libc::geteuid() } != 0 {
        return Err(ChatmailError::config(
            "upgrade must be run as root (sudo) to manage services and replace the binary",
        ));
    }

    let vroot = crate::version_manager::install_root();
    let stable = crate::version_manager::default_stable_binary_path();

    // Prefer version-tree install + pointer flip (no write into versions/<old>/).
    // Probe install root while services are still up; only stop after a successful
    // archive write so a non-writable tree falls back to legacy cleanly.
    match install_into_version_tree(new_bin_path, &version_id, &vroot) {
        Ok(()) => {
            eprintln!(
                "📦 Installed candidate under {}/versions/{version_id}/",
                vroot.display()
            );
            return finish_versioned_upgrade(&version_id, &vroot, &stable, args);
        }
        Err(e) => {
            eprintln!(
                "⚠️ Version tree install skipped ({}): {e} — falling back to in-place replace",
                vroot.display()
            );
        }
    }

    perform_upgrade_legacy_inplace(new_bin_path, &vroot, args)
}

/// Stop services, flip active pointer, smoke, start services (candidate already archived).
fn finish_versioned_upgrade(
    version_id: &str,
    vroot: &Path,
    stable: &Path,
    args: &Args,
) -> Result<()> {
    use crate::version_manager::version_binary_path;

    let installed_bin = version_binary_path(vroot, version_id);

    eprintln!(
        "🚀 Versioned activate: {} (stable entry {})",
        installed_bin.display(),
        stable.display()
    );

    let service = systemd_service_name();
    eprintln!("⏹️ Stopping services...");
    run_systemctl(&["stop", &service]);
    run_systemctl(&["stop", "iroh-relay.service"]);
    thread::sleep(Duration::from_secs(1));

    eprintln!("🔗 Switching active version to {version_id}...");
    let prev = match activate_versioned_install(version_id, vroot, stable) {
        Ok(prev) => prev,
        Err(e) => {
            eprintln!("▶️ Starting services after failed activate...");
            let _ = systemctl_succeeded(&["start", &service]);
            return Err(ChatmailError::config(format!(
                "upgrade rolled back: {e}\n{VARIANT_HINT}"
            )));
        }
    };

    run_post_upgrade_www_migrate(&installed_bin, args);

    eprintln!("▶️ Starting services...");
    let started = systemctl_succeeded(&["start", &service]);
    if !started {
        if preflight_new_binary(&installed_bin, BinaryExecLocation::Installed).is_err() {
            let restore_note = restore_previous_active(vroot, prev.as_deref(), stable);
            let _ = systemctl_succeeded(&["start", &service]);
            return Err(ChatmailError::config(format!(
                "upgrade rolled back: failed to start {service} and binary fails smoke check\n\
                 {restore_note}\n{VARIANT_HINT}"
            )));
        }
        eprintln!(
            "⚠️ Warning: failed to start {service}; the new binary passed smoke checks. \
             Try: systemctl start {service}  (and inspect journalctl -u {service}). \
             Previous version pointer: {:?}.",
            prev
        );
    } else {
        thread::sleep(Duration::from_secs(1));
        if !systemctl_succeeded(&["is-active", "--quiet", &service])
            && preflight_new_binary(&installed_bin, BinaryExecLocation::Installed).is_err()
        {
            let restore_note = restore_previous_active(vroot, prev.as_deref(), stable);
            let _ = systemctl_succeeded(&["start", &service]);
            return Err(ChatmailError::config(format!(
                "upgrade rolled back: {service} is not active and installed binary fails smoke check\n\
                 {restore_note}\n{VARIANT_HINT}"
            )));
        }
    }

    let iroh_unit = PathBuf::from("/etc/systemd/system/iroh-relay.service");
    if iroh_unit.is_file() && !systemctl_succeeded(&["start", "iroh-relay.service"]) {
        eprintln!(
            "⚠️ Warning: failed to start iroh-relay.service; try: systemctl start iroh-relay.service"
        );
    }

    refresh_cli_docs_after_upgrade();
    eprintln!(
        "🎉 Upgrade complete. Active version {version_id} (previous: {:?}).",
        prev
    );
    Ok(())
}

/// Best-effort restore of the previous active version pointer after a failed activate.
fn restore_previous_active(vroot: &Path, prev: Option<&str>, stable: &Path) -> String {
    use crate::version_manager::set_active;
    match prev {
        Some(p) => match set_active(vroot, p, stable) {
            Ok(()) => format!("restored previous version {p}"),
            Err(re) => format!("FAILED to restore previous version {p}: {re}"),
        },
        None => "no previous version pointer to restore".to_string(),
    }
}

/// True if resolving `path` (following symlinks) lands under `{root}/versions/`.
///
/// After the first versioned upgrade, the stable PATH entry is a symlink into the
/// tree, so `canonicalize(stable)` is under `versions/` even though the *entry*
/// lives outside it (e.g. `/usr/local/bin/madmail`).
fn path_resolves_into_version_tree(path: &Path, root: &Path) -> bool {
    let versions = crate::version_manager::versions_dir(root);
    let versions_real = fs::canonicalize(&versions).unwrap_or_else(|_| versions.clone());
    let path_real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    path_real.starts_with(&versions_real)
}

/// True if the path **entry** itself lives under `{root}/versions/` (no follow of
/// the final component). Used to refuse writing *into* the archive tree.
fn path_entry_is_under_version_tree(path: &Path, root: &Path) -> bool {
    let versions = crate::version_manager::versions_dir(root);
    let versions_real = fs::canonicalize(&versions).unwrap_or_else(|_| versions.clone());
    // Absolute-ize without following a final symlink: canonicalize the parent,
    // then join the file name.
    if let Some(parent) = path.parent() {
        if parent.as_os_str().is_empty() {
            return false;
        }
        let parent_real = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        if parent_real.starts_with(&versions_real) {
            return true;
        }
    }
    path.starts_with(&versions) || path.starts_with(&versions_real)
}

/// Choose the filesystem path to replace for a legacy (non-version-tree) upgrade.
///
/// After `set_active`, `current_bin` often resolves into `versions/<old>/`. Writing
/// there clobbers history. Prefer the stable PATH **entry** (symlink/file node)
/// outside the tree so replace unlinks the symlink instead of writing through it.
///
/// Returns an error only when every candidate would write into the version tree.
fn legacy_upgrade_replace_target(
    current_bin: &Path,
    vroot: &Path,
    stable: &Path,
) -> Result<PathBuf> {
    let canonical = fs::canonicalize(current_bin).unwrap_or_else(|_| current_bin.to_path_buf());

    // Safe: replace path is outside the archive tree.
    if !path_resolves_into_version_tree(&canonical, vroot)
        && !path_entry_is_under_version_tree(&canonical, vroot)
    {
        return Ok(canonical);
    }

    // Current exe resolved into versions/ — use stable PATH entry if its *location*
    // is outside the tree (even when it is a symlink whose target is inside).
    if !path_entry_is_under_version_tree(stable, vroot) {
        return Ok(stable.to_path_buf());
    }

    // Last resort: non-canonical current_exe path if it is not under versions/.
    if !path_entry_is_under_version_tree(current_bin, vroot) {
        return Ok(current_bin.to_path_buf());
    }

    Err(ChatmailError::config(format!(
        "refusing to clobber version-tree binary at {}; \
         fix permissions on {} so upgrades can install via install_candidate",
        canonical.display(),
        vroot.display()
    )))
}

/// Flip `current` + stable PATH to `version_id` and smoke-check (no systemd).
///
/// Candidate must already be installed under `versions/<id>/`. Never writes into
/// an older version directory.
fn activate_versioned_install(
    version_id: &str,
    vroot: &Path,
    stable: &Path,
) -> Result<Option<String>> {
    use crate::version_manager::{resolve_active_version, set_active, version_binary_path};

    let installed_bin = version_binary_path(vroot, version_id);
    let prev = resolve_active_version(vroot)?;

    set_active(vroot, version_id, stable).map_err(|e| {
        ChatmailError::config(format!(
            "failed to activate version {version_id} at {}: {e}",
            installed_bin.display()
        ))
    })?;

    if let Err(e) = preflight_new_binary(&installed_bin, BinaryExecLocation::Installed) {
        let restore_note = restore_previous_active(vroot, prev.as_deref(), stable);
        return Err(ChatmailError::config(format!(
            "installed binary failed smoke check: {e} ({restore_note})"
        )));
    }
    Ok(prev)
}

/// Install candidate into the version tree and flip active pointers (no systemd).
///
/// Dual-upgrade-safe core used by tests; production uses install +
/// [`activate_versioned_install`] after the service stop window.
#[cfg(test)]
fn apply_versioned_install_and_activate(
    new_bin_path: &Path,
    version_id: &str,
    vroot: &Path,
    stable: &Path,
) -> Result<Option<String>> {
    install_into_version_tree(new_bin_path, version_id, vroot)?;
    activate_versioned_install(version_id, vroot, stable)
}

/// Replace a PATH entry (file or symlink) with `new_bin` **without** following a
/// symlink into the version tree (unlink the entry first when it is a symlink).
fn replace_path_entry_without_following(new_bin: &Path, dest: &Path) -> Result<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| ChatmailError::config("replace path has no parent"))?;
    fs::create_dir_all(parent)
        .map_err(|e| ChatmailError::config(format!("mkdir {}: {e}", parent.display())))?;
    let tmp = parent.join(format!(".madmail-replace-{}", std::process::id()));
    {
        let mut src = File::open(new_bin)
            .map_err(|e| ChatmailError::config(format!("open {}: {e}", new_bin.display())))?;
        let mut dst = File::create(&tmp)
            .map_err(|e| ChatmailError::config(format!("create {}: {e}", tmp.display())))?;
        io::copy(&mut src, &mut dst)
            .map_err(|e| ChatmailError::config(format!("copy replace staging: {e}")))?;
        dst.sync_all()
            .map_err(|e| ChatmailError::config(format!("sync replace staging: {e}")))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(INSTALLED_EXEC_MODE))
            .map_err(|e| ChatmailError::config(format!("chmod replace staging: {e}")))?;
    }
    // Replace the entry node: if `dest` is a symlink into versions/, remove it
    // first so rename does not write through to the archive.
    if dest
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        fs::remove_file(dest).map_err(|e| {
            ChatmailError::config(format!("unlink symlink {}: {e}", dest.display()))
        })?;
    }
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        ChatmailError::config(format!("replace entry {}: {e}", dest.display()))
    })?;
    Ok(())
}

/// Legacy single-file replace used when the version tree is not writable.
///
/// Refuses to write into `{install_root}/versions/**` (would clobber archives).
fn perform_upgrade_legacy_inplace(new_bin_path: &Path, vroot: &Path, args: &Args) -> Result<()> {
    let current_bin = std::env::current_exe()
        .map_err(|e| ChatmailError::config(format!("failed to get current executable: {e}")))?;
    let stable = crate::version_manager::default_stable_binary_path();
    let real_bin_path = legacy_upgrade_replace_target(&current_bin, vroot, &stable)?;

    eprintln!(
        "🚀 Target binary (legacy in-place): {}",
        real_bin_path.display()
    );

    // Keep a runnable previous binary so a post-replace failure can recover in-band.
    // When real_bin_path is a symlink, copy follows the target for backup contents.
    let backup = backup_current_binary(&real_bin_path)?;

    let service = systemd_service_name();
    eprintln!("⏹️ Stopping services...");
    run_systemctl(&["stop", &service]);
    run_systemctl(&["stop", "iroh-relay.service"]);
    thread::sleep(Duration::from_secs(1));

    eprintln!("🔄 Replacing binary...");
    // Never write through a symlink into versions/<old>/ — unlink the PATH entry
    // first when needed (see replace_path_entry_without_following).
    if let Err(e) = replace_path_entry_without_following(new_bin_path, &real_bin_path) {
        eprintln!("▶️ Starting services after failed replace...");
        let _ = systemctl_succeeded(&["start", &service]);
        return Err(e);
    }

    // Belt-and-suspenders: re-smoke the installed path (catches corrupt write).
    // Uses Installed mode so we do **not** chmod 0700 the live path.
    if let Err(e) = preflight_new_binary(&real_bin_path, BinaryExecLocation::Installed) {
        return rollback_on_broken_install(
            &real_bin_path,
            &backup,
            &service,
            &format!("installed binary failed smoke check: {e}"),
        );
    }

    // Run post-upgrade hooks from the *new* binary so first upgrades that ship
    // html-migrate still work (this process is still the old code).
    run_post_upgrade_www_migrate(&real_bin_path, args);

    eprintln!("▶️ Starting services...");
    let started = systemctl_succeeded(&["start", &service]);
    if !started {
        // Service unit failed — only roll back if the binary itself is broken.
        // Config/port issues must not undo a good ABI-compatible upgrade.
        if preflight_new_binary(&real_bin_path, BinaryExecLocation::Installed).is_err() {
            return rollback_on_broken_install(
                &real_bin_path,
                &backup,
                &service,
                &format!(
                    "failed to start {service} and installed binary no longer passes smoke check"
                ),
            );
        }
        eprintln!(
            "⚠️ Warning: failed to start {service}; the new binary passed smoke checks. \
             Try: systemctl start {service}  (and inspect journalctl -u {service}). \
             Previous binary kept at {}.",
            backup.display()
        );
    } else {
        // Catch crash-loops where systemctl start returns success then the unit dies.
        thread::sleep(Duration::from_secs(1));
        if !systemctl_succeeded(&["is-active", "--quiet", &service])
            && preflight_new_binary(&real_bin_path, BinaryExecLocation::Installed).is_err()
        {
            return rollback_on_broken_install(
                &real_bin_path,
                &backup,
                &service,
                &format!("{service} is not active and installed binary fails smoke check"),
            );
        }
    }

    let iroh_unit = PathBuf::from("/etc/systemd/system/iroh-relay.service");
    if iroh_unit.is_file() && !systemctl_succeeded(&["start", "iroh-relay.service"]) {
        eprintln!(
            "⚠️ Warning: failed to start iroh-relay.service; try: systemctl start iroh-relay.service"
        );
    }

    refresh_cli_docs_after_upgrade();

    eprintln!(
        "🎉 Upgrade complete. (previous binary kept at {} for manual rollback if needed)",
        backup.display()
    );
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
            "ℹ️ --json upgrade: not prompting for www migration. \
             If you use a custom www_dir (Go templates and/or legacy /qr), run: \
             madmail --config {} html-migrate --yes",
            args.config.display()
        );
        return;
    }

    eprintln!("🌐 Checking custom www (Go → Minijinja, legacy /qr → client QR)...");
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
    fn backup_path_for_appends_prev() {
        assert_eq!(
            backup_path_for(Path::new("/usr/local/bin/madmail")),
            PathBuf::from("/usr/local/bin/madmail.prev")
        );
        assert_eq!(
            backup_path_for(Path::new("C:\\Program Files\\Madmail\\madmail.exe")),
            PathBuf::from("C:\\Program Files\\Madmail\\madmail.exe.prev")
        );
    }

    #[cfg(unix)]
    #[test]
    fn preflight_accepts_script_that_prints_version() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("fake-madmail");
        fs::write(&bin, b"#!/bin/sh\necho 'madmail 9.9.9-test'\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o600)).unwrap();
        preflight_new_binary(&bin, BinaryExecLocation::Staging).unwrap();
        let mode = fs::metadata(&bin).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, STAGING_EXEC_MODE, "staging preflight mode {mode:#o}");
    }

    #[cfg(unix)]
    #[test]
    fn preflight_installed_keeps_world_executable() {
        // Regression for #131: must not leave install path as 0700 (systemd User=madmail).
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("madmail");
        fs::write(&bin, b"#!/bin/sh\necho 'madmail 2.20.0'\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o600)).unwrap();
        preflight_new_binary(&bin, BinaryExecLocation::Installed).unwrap();
        let mode = fs::metadata(&bin).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, INSTALLED_EXEC_MODE,
            "installed mode must be 0755, got {mode:#o}"
        );
        assert_ne!(mode, STAGING_EXEC_MODE);
    }

    #[cfg(unix)]
    #[test]
    fn preflight_for_version_manager_keeps_world_executable() {
        // `versions use` must not chmod archived binaries to 0700 (non-root User=).
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("madmail");
        fs::write(&bin, b"#!/bin/sh\necho 'madmail 2.20.0'\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o600)).unwrap();
        preflight_binary_for_version_manager(&bin).unwrap();
        let mode = fs::metadata(&bin).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, INSTALLED_EXEC_MODE,
            "versions use preflight must leave 0755, got {mode:#o}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn preflight_rejects_binary_that_exits_nonzero() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("broken-madmail");
        fs::write(
            &bin,
            b"#!/bin/sh\necho \"version 'GLIBC_2.39' not found\" >&2\nexit 127\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        let err = preflight_new_binary(&bin, BinaryExecLocation::Staging).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("host check") || msg.contains("GLIBC") || msg.contains("failed"),
            "got: {msg}"
        );
        assert!(
            msg.contains("legacy") || msg.contains("NOT replaced"),
            "expected recovery/variant hint, got: {msg}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn backup_and_restore_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("madmail");
        fs::write(&current, b"old-binary-bytes").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&current, fs::Permissions::from_mode(0o755)).unwrap();

        let backup = backup_current_binary(&current).unwrap();
        assert!(backup.ends_with("madmail.prev"));
        assert_eq!(fs::read(&backup).unwrap(), b"old-binary-bytes");
        let bak_mode = fs::metadata(&backup).unwrap().permissions().mode() & 0o777;
        assert_eq!(bak_mode, 0o755, "backup should keep live mode");

        fs::write(&current, b"new-broken-bytes").unwrap();
        restore_backup(&backup, &current).unwrap();
        assert_eq!(fs::read(&current).unwrap(), b"old-binary-bytes");
        let live_mode = fs::metadata(&current).unwrap().permissions().mode() & 0o777;
        assert_eq!(live_mode, INSTALLED_EXEC_MODE);
    }

    /// Restore must force 0755 even when `*.prev` was copied from a broken 0700 install.
    #[cfg(unix)]
    #[test]
    fn restore_backup_forces_world_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("madmail");
        // Script so we can actually exec after restore.
        fs::write(&current, b"#!/bin/sh\necho old-version\n").unwrap();
        fs::set_permissions(&current, fs::Permissions::from_mode(0o700)).unwrap();

        let backup = backup_current_binary(&current).unwrap();
        let bak_mode = fs::metadata(&backup).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            bak_mode, 0o700,
            "backup copies source mode including broken 0700"
        );

        fs::write(&current, b"#!/bin/sh\necho new-broken\n").unwrap();
        fs::set_permissions(&current, fs::Permissions::from_mode(0o700)).unwrap();

        restore_backup(&backup, &current).unwrap();
        assert_eq!(
            fs::read(&current).unwrap(),
            b"#!/bin/sh\necho old-version\n"
        );
        let live_mode = fs::metadata(&current).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            live_mode, INSTALLED_EXEC_MODE,
            "restore must leave install path 0755 for User=madmail, got {live_mode:#o}"
        );
        let out = std::process::Command::new(&current)
            .output()
            .expect("exec restored binary");
        assert!(out.status.success(), "restored binary must be executable");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("old-version"),
            "stdout={:?}",
            String::from_utf8_lossy(&out.stdout)
        );
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
        let err = upgrade_command("  ", &test_args(), false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("PATH") || msg.contains("required") || msg.contains("latest"),
            "got: {msg}"
        );
    }

    #[test]
    fn upgrade_latest_keyword_resolves_github_url() {
        let url = crate::version_manager::github_latest_asset_url();
        assert!(url.starts_with("https://github.com/themadorg/madmail/releases/latest/download/"));
        assert!(url.contains("madmail"));
        // Keyword is handled before local-path open; empty/whitespace still rejected.
        assert!(upgrade_command("  ", &test_args(), false).is_err());
    }

    #[test]
    fn capture_version_id_from_script() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("madmail");
        fs::write(&bin, b"#!/bin/sh\necho madmail-v2 4.5.6\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(capture_version_id(&bin), "4.5.6");
    }

    #[test]
    fn install_into_version_tree_writes_binary_and_meta() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("opt-madmail");
        let src = dir.path().join("payload");
        fs::write(&src, b"#!/bin/sh\necho madmail-v2 1.2.3\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();
        }
        install_into_version_tree(&src, "1.2.3", &root).unwrap();
        let bin = crate::version_manager::version_binary_path(&root, "1.2.3");
        assert!(bin.is_file());
        let meta = crate::version_manager::read_meta(&root, "1.2.3")
            .unwrap()
            .expect("meta");
        assert_eq!(meta.version, "1.2.3");
        assert_eq!(meta.signature_ok, Some(true));
        assert_eq!(meta.source.as_deref(), Some("upgrade"));
    }

    fn write_version_script(path: &Path, version: &str, marker: &str) {
        let body = format!(
            "#!/bin/sh\nif [ \"$1\" = version ]; then echo madmail-v2 {version}; fi\necho {marker}\nexit 0\n"
        );
        fs::write(path, body.as_bytes()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn path_tree_helpers_distinguish_symlink_entry_from_target() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("opt");
        let versions = crate::version_manager::versions_dir(&root);
        let archived = versions.join("2.18.2").join("madmail");
        fs::create_dir_all(archived.parent().unwrap()).unwrap();
        fs::write(&archived, b"old").unwrap();

        assert!(path_resolves_into_version_tree(&archived, &root));
        assert!(path_entry_is_under_version_tree(&archived, &root));

        let outside = dir.path().join("outside-madmail");
        fs::write(&outside, b"live").unwrap();
        assert!(!path_resolves_into_version_tree(&outside, &root));
        assert!(!path_entry_is_under_version_tree(&outside, &root));

        // Stable PATH symlink into the tree: resolves into tree, but entry is outside.
        let stable = dir.path().join("bin").join("madmail");
        fs::create_dir_all(stable.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&archived, &stable).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&archived, &stable).unwrap();

        assert!(
            path_resolves_into_version_tree(&stable, &root),
            "canonicalize(stable) must land under versions/"
        );
        assert!(
            !path_entry_is_under_version_tree(&stable, &root),
            "stable PATH entry itself must not be considered under versions/"
        );
    }

    /// Full dual-upgrade regression for review #136:
    /// 1) install+activate v1 (PATH becomes symlink into versions/v1)
    /// 2) install+activate v2 via the same core as perform_upgrade
    /// 3) v1 bytes must stay bit-identical
    /// 4) legacy replace target must be the stable *entry*, not versions/v1
    /// 5) legacy replace of the stable entry must not clobber v1
    /// 6) document that write-through canonicalize(stable) would destroy v1
    #[test]
    fn dual_upgrade_preserves_prior_archive_and_legacy_target() {
        use crate::version_manager::{resolve_active_version, version_binary_path};

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("opt-madmail");
        let stable = dir.path().join("usr-local-bin").join("madmail");
        fs::create_dir_all(stable.parent().unwrap()).unwrap();

        let v1_src = dir.path().join("payload-v1");
        let v2_src = dir.path().join("payload-v2");
        write_version_script(&v1_src, "2.18.2", "MARKER-V1");
        write_version_script(&v2_src, "2.20.0", "MARKER-V2");

        // --- Upgrade #1 (versioned core) ---
        let prev1 =
            apply_versioned_install_and_activate(&v1_src, "2.18.2", &root, &stable).unwrap();
        assert_eq!(prev1, None);
        assert_eq!(
            resolve_active_version(&root).unwrap().as_deref(),
            Some("2.18.2")
        );
        let v1_path = version_binary_path(&root, "2.18.2");
        let v1_bytes = fs::read(&v1_path).unwrap();
        assert!(
            String::from_utf8_lossy(&v1_bytes).contains("MARKER-V1"),
            "v1 archive should contain marker"
        );

        // After activate, PATH entry is a symlink into the version tree — the
        // dangerous canonicalize target that the old upgrade used.
        let meta = fs::symlink_metadata(&stable).unwrap();
        assert!(
            meta.file_type().is_symlink(),
            "stable PATH must be a symlink after set_active"
        );
        let clobber_target = fs::canonicalize(&stable).unwrap();
        assert!(
            path_resolves_into_version_tree(&clobber_target, &root),
            "canonicalize(stable) should resolve under versions/"
        );
        assert_eq!(
            clobber_target,
            fs::canonicalize(&v1_path).unwrap(),
            "stable should resolve to the v1 archive binary"
        );

        // Legacy replace target must be the stable entry, NOT the archive path.
        let replace_target = legacy_upgrade_replace_target(&stable, &root, &stable).unwrap();
        assert_eq!(
            replace_target, stable,
            "must replace the PATH entry, not versions/<old>/madmail"
        );
        assert!(
            !path_entry_is_under_version_tree(&replace_target, &root),
            "replace target entry must live outside the version tree"
        );

        // --- Document the old bug: write-through would destroy v1 ---
        // (do not leave this state — restore immediately after the assert setup)
        let destroyed_probe = dir.path().join("would-clobber");
        fs::write(&destroyed_probe, b"V2-OVERWRITE").unwrap();
        // Simulate old code: copy onto canonicalize(stable)
        fs::copy(&destroyed_probe, &clobber_target).unwrap();
        assert_ne!(
            fs::read(&v1_path).unwrap(),
            v1_bytes,
            "sanity: writing through canonicalize(stable) must clobber v1"
        );
        // Restore v1 archive for the real dual-upgrade path
        fs::write(&v1_path, &v1_bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&v1_path, fs::Permissions::from_mode(0o755)).unwrap();
        }

        // --- Upgrade #2 via versioned core (install_candidate + set_active) ---
        let prev2 =
            apply_versioned_install_and_activate(&v2_src, "2.20.0", &root, &stable).unwrap();
        assert_eq!(prev2.as_deref(), Some("2.18.2"));
        assert_eq!(
            resolve_active_version(&root).unwrap().as_deref(),
            Some("2.20.0")
        );

        assert_eq!(
            fs::read(&v1_path).unwrap(),
            v1_bytes,
            "versions/2.18.2 must remain bit-identical after installing 2.20.0"
        );
        let v2_path = version_binary_path(&root, "2.20.0");
        assert!(
            String::from_utf8_lossy(&fs::read(&v2_path).unwrap()).contains("MARKER-V2"),
            "v2 archive should contain its own marker"
        );
        let real = fs::canonicalize(&stable).unwrap();
        assert!(
            real.to_string_lossy().contains("2.20.0"),
            "stable should now resolve into 2.20.0, got {}",
            real.display()
        );

        // --- Legacy replace of stable entry must not touch either archive ---
        let v1_after = fs::read(&v1_path).unwrap();
        let v2_after = fs::read(&v2_path).unwrap();
        let v3_src = dir.path().join("payload-v3");
        write_version_script(&v3_src, "2.21.0", "MARKER-LEGACY");
        let target = legacy_upgrade_replace_target(&stable, &root, &stable).unwrap();
        replace_path_entry_without_following(&v3_src, &target).unwrap();
        assert_eq!(
            fs::read(&v1_path).unwrap(),
            v1_after,
            "legacy replace must not clobber versions/2.18.2"
        );
        assert_eq!(
            fs::read(&v2_path).unwrap(),
            v2_after,
            "legacy replace must not clobber versions/2.20.0"
        );
        // Stable is now a regular file (symlink replaced), not a version-tree path.
        let stable_meta = fs::symlink_metadata(&stable).unwrap();
        assert!(
            !stable_meta.file_type().is_symlink(),
            "legacy replace should leave a regular file at the PATH entry"
        );
        assert!(
            String::from_utf8_lossy(&fs::read(&stable).unwrap()).contains("MARKER-LEGACY"),
            "stable entry should hold the legacy-replaced binary"
        );
    }

    #[test]
    fn legacy_target_refuses_when_only_version_tree_paths_available() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("opt");
        let versions = crate::version_manager::versions_dir(&root);
        let archived = versions.join("1.0.0").join("madmail");
        fs::create_dir_all(archived.parent().unwrap()).unwrap();
        fs::write(&archived, b"x").unwrap();
        // Both current and stable live under versions/ → refuse.
        let err = legacy_upgrade_replace_target(&archived, &root, &archived).unwrap_err();
        assert!(
            err.to_string().contains("refusing to clobber"),
            "got: {err}"
        );
    }

    #[test]
    fn restore_previous_active_reports_honest_status() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("opt");
        let stable = dir.path().join("bin").join("madmail");
        // No previous → honest message
        let note = restore_previous_active(&root, None, &stable);
        assert!(note.contains("no previous"), "got: {note}");
        // Missing prev version → failed restore, not "restored"
        let note = restore_previous_active(&root, Some("9.9.9"), &stable);
        assert!(
            note.contains("FAILED") || note.contains("not found"),
            "got: {note}"
        );
        assert!(!note.starts_with("restored previous"), "got: {note}");
    }

    #[test]
    fn unsigned_binary_fails_verify_before_version_tree() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("madmail");
        // Large enough to not trip "too small for signature", but unsigned.
        let mut payload = vec![0u8; 128];
        payload[..8].copy_from_slice(b"unsigned");
        fs::write(&bin, &payload).unwrap();
        assert!(!verify_signature(&bin).unwrap());
    }

    #[test]
    fn signed_binary_passes_verify_for_version_activation() {
        let Some(key) = official_private_key_path() else {
            eprintln!("skip: official private key not available");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("madmail");
        // Script body + signature trailer; shell ignores trailing bytes after script.
        fs::write(
            &bin,
            b"#!/bin/sh\nif [ \"$1\" = version ]; then echo madmail-v2 0.0.1; fi\nexit 0\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        }
        sign_with_official_key(&bin, &key);
        assert!(verify_signature(&bin).unwrap());
        preflight_binary_for_version_manager(&bin).unwrap();

        // Install into tree and confirm signature still verifies on archived copy.
        let root = dir.path().join("root");
        install_into_version_tree(&bin, "0.0.1", &root).unwrap();
        let archived = crate::version_manager::version_binary_path(&root, "0.0.1");
        assert!(verify_signature(&archived).unwrap());
    }

    #[test]
    fn upgrade_command_rejects_zip_url_without_download() {
        // Must fail before any network I/O.
        let err =
            upgrade_command("https://example.com/madmail.zip", &test_args(), false).unwrap_err();
        assert!(
            err.to_string().contains("unsupported archive format"),
            "got: {err}"
        );
    }

    #[test]
    fn upgrade_command_rejects_tar_bz2_url_without_download() {
        let err = upgrade_command("https://example.com/madmail.tar.bz2", &test_args(), false)
            .unwrap_err();
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
        assert!(!is_safe_tar_member("evil\0madmail"));
        assert!(!is_safe_tar_member("foo\\bar"));
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

        let err = upgrade_command(&url, &test_args(), false).unwrap_err();
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

        let err = upgrade_command(&url, &test_args(), false).unwrap_err();
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
        let err = upgrade_command(bin.to_str().unwrap(), &test_args(), false).unwrap_err();
        assert!(err.to_string().contains("INVALID SIGNATURE"), "got: {err}");
    }

    /// When the official signing key is available, prove signed `madmail` inside
    /// `.tar.gz` passes verification (the full traditional check) after extract.
    ///
    /// Payload is a tiny shell script that implements `version` so host preflight
    /// (issue #114) also succeeds; trailing signature bytes are fine because the
    /// script `exit 0`s before the interpreter would see them.
    #[test]
    fn signed_madmail_inside_tar_gz_passes_verify_after_extract() {
        let Some(key_path) = official_private_key_path() else {
            eprintln!("skip: official private key not found");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("madmail");
        fs::write(
            &payload,
            b"#!/bin/sh\nif [ \"$1\" = version ]; then echo madmail-test-signed 0.0.0; fi\nexit 0\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&payload, fs::Permissions::from_mode(0o755)).unwrap();
        }
        sign_with_official_key(&payload, &key_path);

        assert!(
            verify_signature(&payload).unwrap(),
            "signed payload must verify before packaging"
        );
        // Preflight must accept this signed runnable payload (not just signature).
        preflight_new_binary(&payload, BinaryExecLocation::Staging).unwrap();

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

        // Full URL pipeline: download .tar.gz → extract → perform_upgrade verify + preflight.
        let body = fs::read(&archive).unwrap();
        let (url, server) = serve_once(body);
        let err = upgrade_command(&url, &test_args(), false).unwrap_err();
        let msg = err.to_string();
        // Signature + preflight OK; non-root should fail before replace (or root-only env).
        assert!(
            msg.contains("must be run as root") || msg.contains("Upgrade complete"),
            "expected post-signature/preflight traditional path, got: {msg}"
        );
        server.join().unwrap();
    }

    /// Signed but non-executable payload must fail preflight *before* root/replace
    /// (the whole point of issue #114 — never brick on a bad variant).
    #[test]
    fn signed_non_executable_fails_preflight_not_root_check() {
        let Some(key_path) = official_private_key_path() else {
            eprintln!("skip: official private key not found");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("madmail");
        fs::write(
            &payload,
            b"MADMAIL_NOT_AN_ELF_JUST_BYTES_FOR_SIGNATURE_ONLY_0123456789",
        )
        .unwrap();
        sign_with_official_key(&payload, &key_path);
        assert!(verify_signature(&payload).unwrap());

        let err = perform_upgrade(&payload, &test_args()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("preflight")
                || msg.contains("failed to execute")
                || msg.contains("NOT replaced")
                || msg.contains("legacy"),
            "expected preflight abort, got: {msg}"
        );
        assert!(
            !msg.contains("must be run as root"),
            "must not reach root check after failed preflight: {msg}"
        );
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
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/publish/sign.py"))
            .arg(file)
            .arg(key_path)
            .status()
            .expect("run sign.py");
        assert!(status.success(), "sign.py failed: {status}");
    }

    #[test]
    fn allow_unsafe_tls_respects_accept_unsafe_https_flag() {
        assert!(allow_unsafe_tls(true, &test_args()).unwrap());
    }

    #[test]
    fn allow_unsafe_tls_errors_without_flag_when_noninteractive() {
        // Tests run without a TTY stdin → must not hang; require --accept-unsafe-https.
        let err = allow_unsafe_tls(false, &test_args()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("--accept-unsafe-https"),
            "expected --accept-unsafe-https guidance, got: {msg}"
        );
    }

    #[test]
    fn allow_unsafe_tls_errors_under_json_without_flag() {
        let mut args = test_args();
        args.json = true;
        let err = allow_unsafe_tls(false, &args).unwrap_err();
        assert!(err.to_string().contains("--accept-unsafe-https"));
    }

    #[test]
    fn is_tls_certificate_error_detects_common_phrases() {
        assert!(tls_error_blob_matches(
            "error sending request for url error trying to connect invalid peer certificate: UnknownIssuer"
        ));
        assert!(tls_error_blob_matches(
            "connection failed: self-signed certificate in certificate chain"
        ));
        assert!(!tls_error_blob_matches(
            "error sending request: connection refused"
        ));
    }

    /// One-shot Python HTTPS server with a self-signed cert (single GET).
    fn serve_https_once(body: &[u8]) -> Option<(String, tempfile::TempDir, std::process::Child)> {
        let dir = tempfile::tempdir().ok()?;
        let cert = dir.path().join("cert.pem");
        let key = dir.path().join("key.pem");
        let body_path = dir.path().join("payload.bin");
        fs::write(&body_path, body).ok()?;

        let status = Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-keyout",
                key.to_str()?,
                "-out",
                cert.to_str()?,
                "-days",
                "1",
                "-nodes",
                "-subj",
                "/CN=localhost",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }

        let listener = TcpListener::bind("127.0.0.1:0").ok()?;
        let port = listener.local_addr().ok()?.port();
        drop(listener);

        let script = dir.path().join("serve.py");
        fs::write(
            &script,
            format!(
                r#"
import http.server, ssl
from pathlib import Path
port = {port}
body = Path({body:?}).read_bytes()
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a):
        pass
httpd = http.server.HTTPServer(("127.0.0.1", port), H)
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ctx.load_cert_chain({cert:?}, {key:?})
httpd.socket = ctx.wrap_socket(httpd.socket, server_side=True)
httpd.handle_request()
"#,
                port = port,
                body = body_path,
                cert = cert,
                key = key,
            ),
        )
        .ok()?;

        let child = Command::new("python3")
            .arg(&script)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        // Wait for the port to be bound. Do NOT TcpStream::connect: that would
        // consume the one-shot HTTPServer accept without completing TLS.
        for _ in 0..100 {
            if TcpListener::bind(("127.0.0.1", port)).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        thread::sleep(Duration::from_millis(80));
        Some((format!("https://127.0.0.1:{port}/madmail"), dir, child))
    }

    #[test]
    fn https_self_signed_fails_without_accept_unsafe_https() {
        let Some((url, _dir, mut child)) = serve_https_once(b"unsigned-payload-bytes-xxxxxxxxxxxx")
        else {
            eprintln!("skip: openssl/python HTTPS harness unavailable");
            return;
        };
        let err = upgrade_command(&url, &test_args(), false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("--accept-unsafe-https")
                || msg.to_ascii_lowercase().contains("certificate")
                || msg.to_ascii_lowercase().contains("tls"),
            "expected TLS rejection guidance, got: {msg}"
        );
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn https_self_signed_downloads_with_accept_unsafe_https() {
        // --accept-unsafe-https uses unsafe client immediately (one request).
        let body = vec![b'S'; 128];
        let Some((url, _dir, mut child)) = serve_https_once(&body) else {
            eprintln!("skip: openssl/python HTTPS harness unavailable");
            return;
        };
        let err = upgrade_command(&url, &test_args(), true).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("INVALID SIGNATURE"),
            "expected download+signature path with --accept-unsafe-https, got: {msg}"
        );
        let _ = child.kill();
        let _ = child.wait();
    }
}
