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

use std::path::{Path, PathBuf};

use chatmail_acme::generate_self_signed;
use chatmail_config::{effective_tls_pem_paths, AppConfig};
use chatmail_types::{ChatmailError, Result};

/// Ensure cert/key PEMs exist for listeners that need TLS.
///
/// When files are missing:
/// - `tls_mode self_signed`, or bare defaults (no mode and no explicit `tls file` paths):
///   generate a self-signed pair under the effective PEM paths.
/// - `autocert` / `file` / explicit paths: return a configuration error with next steps.
pub fn ensure_tls_pem_files(config: &AppConfig, state_dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let (cert, key) = effective_tls_pem_paths(config, state_dir);
    if cert.is_file() && key.is_file() {
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

    Err(missing_tls_error(&cert, &key))
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
    s.trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_string()
}

fn missing_tls_error(cert: &Path, key: &Path) -> ChatmailError {
    ChatmailError::config(format!(
        "TLS certificate not found: {}\n  private key: {}\n\n\
         Madmail needs PEM files before SMTP/IMAP/HTTPS listeners can start.\n\n\
         First-time / local setup (self-signed):\n\
           madmail install --simple --ip <YOUR_IP_OR_127.0.0.1> --tls-mode self_signed --lang en\n\
         On Windows, defaults write under %ProgramData%\\Madmail (no Unix FHS paths).\n\
         Prefer the Windows setup wizard when available.\n\n\
         Then start:\n\
           madmail --config <config> run --libexec <state-dir>\n\n\
         Let's Encrypt (public IP/domain, port 80 free):\n\
           madmail install --simple --ip <PUBLIC_IP> --auto-ip-cert --acme-email you@example.com\n\
           # or: madmail certificate get\n\n\
         Or place fullchain.pem + privkey.pem at the paths above.",
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
    }
}
