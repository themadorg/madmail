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

//! Bootstrap TLS PEMs for first-run / `tls_mode self_signed`, with actionable errors.

use std::fs;
use std::path::{Path, PathBuf};

use chatmail_acme::generate_self_signed;
use chatmail_config::{effective_tls_pem_paths, AppConfig};
use chatmail_types::{ChatmailError, Result};

/// Ensure cert/key PEMs exist for listeners that need TLS.
///
/// Paths may be **symlinks** (common for ACME / Let's Encrypt under `/etc/ssl/…`).
/// Existence checks follow links to the target; the configured path is kept as-is so
/// renewals that rewrite the symlink target keep working without a config change.
///
/// When files are missing:
/// - `tls_mode self_signed`, or bare defaults (no mode and no explicit `tls file` paths):
///   generate a self-signed pair under the effective PEM paths.
/// - `autocert` / `file` / explicit paths: return a configuration error with next steps.
pub fn ensure_tls_pem_files(config: &AppConfig, state_dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let (cert, key) = effective_tls_pem_paths(config, state_dir);
    let cert_ok = pem_path_ready(&cert);
    let key_ok = pem_path_ready(&key);
    if cert_ok.is_ok() && key_ok.is_ok() {
        return Ok((cert, key));
    }

    if should_bootstrap_self_signed(config) {
        let domain = identity_name(config);
        let hostname = config
            .hostname
            .as_deref()
            .map(strip_brackets)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| domain.clone());
        let public_ip = config
            .public_ip
            .as_deref()
            .map(strip_brackets)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| domain.clone());

        eprintln!(
            "TLS certificates missing; generating self-signed certificate for {domain}\n  cert: {}\n  key:  {}",
            cert.display(),
            key.display()
        );
        generate_self_signed(&domain, &hostname, &public_ip, &cert, &key)?;
        return Ok((cert, key));
    }

    Err(missing_tls_error(&cert, &key, cert_ok.err(), key_ok.err()))
}

/// True when `path` is a readable regular file, **following symlinks**.
///
/// Returns `Err` with a short reason when the path is missing, a broken link,
/// not a file, or unreadable (e.g. permission denied on a symlink target).
pub fn pem_path_ready(path: &Path) -> std::result::Result<(), String> {
    // Prefer open (follows symlinks) so we match what rustls will do at load time.
    match fs::File::open(path) {
        Ok(_) => Ok(()),
        Err(e) => {
            let link_note = describe_symlink(path);
            Err(format!("{} ({e}){link_note}", path.display()))
        }
    }
}

fn describe_symlink(path: &Path) -> String {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => match fs::read_link(path) {
            Ok(target) => format!(
                "\n  (symlink → {}; ensure the target exists and is readable by the madmail process)",
                target.display()
            ),
            Err(_) => "\n  (path is a symlink but the target could not be read)".into(),
        },
        _ => String::new(),
    }
}

/// Bootstrap when operators chose self-signed, or when running with no TLS mode and
/// no explicit PEM paths (typical first double-click / bare `madmail run`).
pub fn should_bootstrap_self_signed(config: &AppConfig) -> bool {
    match config
        .tls_mode
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some("self_signed") => true,
        Some(_) => false,
        None => config.tls_cert_path.is_none() && config.tls_key_path.is_none(),
    }
}

fn identity_name(config: &AppConfig) -> String {
    config
        .primary_domain
        .as_deref()
        .or(config.hostname.as_deref())
        .or(config.public_ip.as_deref())
        .map(strip_brackets)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "127.0.0.1".into())
}

fn strip_brackets(s: &str) -> String {
    s.trim().trim_matches(|c| c == '[' || c == ']').to_string()
}

fn missing_tls_error(
    cert: &Path,
    key: &Path,
    cert_detail: Option<String>,
    key_detail: Option<String>,
) -> ChatmailError {
    let mut detail = String::new();
    if let Some(d) = cert_detail {
        detail.push_str(&format!("\nCertificate: {d}"));
    }
    if let Some(d) = key_detail {
        detail.push_str(&format!("\nPrivate key: {d}"));
    }
    ChatmailError::config(format!(
        "TLS certificate not found: {}\n  private key: {}{detail}\n\n\
         Madmail needs PEM files before SMTP/IMAP/HTTPS listeners can start.\n\
         Symlinks are supported (e.g. ACME links under /etc/ssl/); the target must exist\n\
         and be readable by the madmail process (service user / OpenWrt procd user).\n\n\
         First-time / local setup (self-signed):\n\
           madmail install --simple --ip <YOUR_IP_OR_127.0.0.1> --tls-mode self_signed --lang en\n\
         On Windows, defaults write under %ProgramData%\\Madmail (no Unix FHS paths).\n\
         Prefer the Windows setup wizard when available.\n\n\
         Then start:\n\
           madmail --config <config> run --libexec <state-dir>\n\n\
         Let's Encrypt (public IP/domain, port 80 free):\n\
           madmail install --simple --ip <PUBLIC_IP> --auto-ip-cert --acme-email you@example.com\n\
           # or: madmail certificate get\n\n\
         Or place fullchain.pem + privkey.pem at the paths above (or point tls file at\n\
         durable symlinks that always resolve to the current PEMs).",
        cert.display(),
        key.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_tls::load_server_config;

    #[test]
    fn bootstrap_when_self_signed_mode() {
        let cfg = AppConfig {
            tls_mode: Some("self_signed".into()),
            ..Default::default()
        };
        assert!(should_bootstrap_self_signed(&cfg));
    }

    #[test]
    fn bootstrap_when_no_tls_mode_and_no_explicit_paths() {
        assert!(should_bootstrap_self_signed(&AppConfig::default()));
    }

    #[test]
    fn no_bootstrap_for_autocert_or_file() {
        for mode in ["autocert", "file"] {
            let cfg = AppConfig {
                tls_mode: Some(mode.into()),
                ..Default::default()
            };
            assert!(!should_bootstrap_self_signed(&cfg), "mode={mode}");
        }
    }

    #[test]
    fn no_bootstrap_when_explicit_pem_paths_without_mode() {
        let cfg = AppConfig {
            tls_cert_path: Some(PathBuf::from("/etc/madmail/certs/fullchain.pem")),
            tls_key_path: Some(PathBuf::from("/etc/madmail/certs/privkey.pem")),
            ..Default::default()
        };
        assert!(!should_bootstrap_self_signed(&cfg));
    }

    #[test]
    fn ensure_generates_loadable_self_signed() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = AppConfig {
            tls_mode: Some("self_signed".into()),
            hostname: Some("127.0.0.1".into()),
            primary_domain: Some("127.0.0.1".into()),
            ..Default::default()
        };
        let (cert, key) = ensure_tls_pem_files(&cfg, dir.path()).unwrap();
        assert!(cert.is_file());
        assert!(key.is_file());
        load_server_config(&cert, &key).unwrap();
        // Second call is a no-op load path.
        let (cert2, key2) = ensure_tls_pem_files(&cfg, dir.path()).unwrap();
        assert_eq!(cert, cert2);
        assert_eq!(key, key2);
    }

    #[test]
    fn ensure_errors_helpfully_for_file_mode() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = AppConfig {
            tls_mode: Some("file".into()),
            ..Default::default()
        };
        let err = ensure_tls_pem_files(&cfg, dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("TLS certificate not found"), "{msg}");
        assert!(msg.contains("install --simple"), "{msg}");
        assert!(msg.contains("Windows"), "{msg}");
        assert!(msg.contains("Symlinks are supported"), "{msg}");
    }

    #[cfg(unix)]
    #[test]
    fn ensure_accepts_symlinked_pem_paths() {
        // Operators often point `tls file` at /etc/ssl/acme/*.crt symlinks (#133).
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        let link_dir = dir.path().join("ssl");
        fs::create_dir_all(&real).unwrap();
        fs::create_dir_all(&link_dir).unwrap();

        let cfg_gen = AppConfig {
            tls_mode: Some("self_signed".into()),
            hostname: Some("127.0.0.1".into()),
            primary_domain: Some("127.0.0.1".into()),
            ..Default::default()
        };
        let (cert_real, key_real) = ensure_tls_pem_files(&cfg_gen, &real).unwrap();

        let cert_link = link_dir.join("fullchain.crt");
        let key_link = link_dir.join("privkey.key");
        std::os::unix::fs::symlink(&cert_real, &cert_link).unwrap();
        std::os::unix::fs::symlink(&key_real, &key_link).unwrap();
        assert!(cert_link.is_symlink());
        assert!(key_link.is_symlink());

        let cfg = AppConfig {
            tls_mode: Some("file".into()),
            tls_cert_path: Some(cert_link.clone()),
            tls_key_path: Some(key_link.clone()),
            ..Default::default()
        };
        let (c, k) = ensure_tls_pem_files(&cfg, dir.path()).unwrap();
        // Config paths preserved (symlink paths), not rewritten to realpath.
        assert_eq!(c, cert_link);
        assert_eq!(k, key_link);
        load_server_config(&c, &k).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ensure_reports_broken_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("missing.crt");
        let key_link = dir.path().join("missing.key");
        std::os::unix::fs::symlink(dir.path().join("no-such-cert.pem"), &link).unwrap();
        std::os::unix::fs::symlink(dir.path().join("no-such-key.pem"), &key_link).unwrap();

        let cfg = AppConfig {
            tls_mode: Some("file".into()),
            tls_cert_path: Some(link),
            tls_key_path: Some(key_link),
            ..Default::default()
        };
        let err = ensure_tls_pem_files(&cfg, dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("TLS certificate not found"), "{msg}");
        assert!(
            msg.contains("symlink") || msg.contains("No such file"),
            "{msg}"
        );
    }
}
