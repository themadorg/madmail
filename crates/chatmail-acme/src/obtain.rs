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

//! Obtain / renew Let's Encrypt certificates via [lers](https://github.com/akrantz01/lers) (DNS)
//! or [instant-acme](https://github.com/djc/instant-acme) (public IP, short-lived profile).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use chatmail_types::{ChatmailError, Result};
use lers::solver::Http01Solver;
use lers::{Directory, LETS_ENCRYPT_PRODUCTION_URL, LETS_ENCRYPT_STAGING_URL};
use openssl::ec::{EcGroup, EcKey};
use openssl::nid::Nid;
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use tracing::info;

use crate::obtain_ip::{obtain_ip_certificate, parse_public_ip};

/// Options for `certificate get` / `regenerate` and install-time issuance.
#[derive(Debug, Clone)]
pub struct ObtainOptions {
    pub domain: String,
    pub email: String,
    pub state_dir: PathBuf,
    /// PEM output paths (default: `{state_dir}/certs/`).
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    /// Listen address for HTTP-01 (default `0.0.0.0:80`).
    pub http_listen: SocketAddr,
    pub staging: bool,
    /// Skip issuance when an existing cert is still valid (`get` only).
    pub skip_if_valid: bool,
}

/// Renewal threshold for normal 90-day DNS certificates.
pub const DNS_CERT_RENEW_WITHIN_DAYS: u32 = 30;

impl ObtainOptions {
    pub fn cert_path(&self) -> PathBuf {
        self.cert_path
            .clone()
            .unwrap_or_else(|| self.state_dir.join("certs/fullchain.pem"))
    }

    pub fn key_path(&self) -> PathBuf {
        self.key_path
            .clone()
            .unwrap_or_else(|| self.state_dir.join("certs/privkey.pem"))
    }

    pub fn account_key_path(&self) -> PathBuf {
        self.state_dir.join("autocert/account.key.pem")
    }

    pub fn le_account_path(&self) -> PathBuf {
        self.state_dir.join("autocert/le-account.json")
    }
}

/// Obtain (or renew) a certificate and write PEM files.
pub async fn obtain_certificate(opts: &ObtainOptions) -> Result<()> {
    if parse_public_ip(&opts.domain).is_ok() {
        return obtain_ip_certificate(opts).await;
    }
    obtain_dns_certificate(opts).await
}

async fn obtain_dns_certificate(opts: &ObtainOptions) -> Result<()> {
    let domain = normalize_acme_domain(&opts.domain)?;
    if opts.skip_if_valid
        && !cert_needs_renewal(&opts.cert_path(), DNS_CERT_RENEW_WITHIN_DAYS)?
    {
        info!(
            cert = %opts.cert_path().display(),
            "existing certificate still valid; skipping issuance"
        );
        println!(
            "Certificate at {} is still valid (≥{DNS_CERT_RENEW_WITHIN_DAYS} days left). Use `certificate regenerate` to force renewal.",
            opts.cert_path().display()
        );
        return Ok(());
    }

    println!("Starting HTTP-01 challenge listener on {}…", opts.http_listen);
    let solver = Http01Solver::new();
    let handle = solver
        .start(&opts.http_listen)
        .map_err(|e| ChatmailError::config(format!("bind HTTP-01 on {}: {e}", opts.http_listen)))?;

    let directory_url = if opts.staging {
        LETS_ENCRYPT_STAGING_URL
    } else {
        LETS_ENCRYPT_PRODUCTION_URL
    };

    let directory = Directory::builder(directory_url.to_string())
        .http01_solver(Box::new(solver))
        .build()
        .await
        .map_err(map_lers)?;

    let account_key = load_or_generate_account_key(&opts.account_key_path())?;
    let contact = format!("mailto:{}", opts.email);

    let account = directory
        .account()
        .private_key(account_key.clone())
        .terms_of_service_agreed(true)
        .contacts(vec![contact])
        .create_if_not_exists()
        .await
        .map_err(map_lers)?;

    save_account_key(&opts.account_key_path(), account.private_key())?;

    println!("Requesting certificate for {domain} from Let's Encrypt…");
    let certificate = account
        .certificate()
        .add_domain(&domain)
        .obtain()
        .await
        .map_err(map_lers)?;

    write_lers_certificate(&opts.cert_path(), &opts.key_path(), &certificate)?;

    handle
        .stop()
        .await
        .map_err(|e| ChatmailError::config(format!("stop HTTP-01 listener: {e}")))?;

    println!(
        "✓ Certificate written:\n  {}\n  {}",
        opts.cert_path().display(),
        opts.key_path().display()
    );
    Ok(())
}

fn normalize_acme_domain(domain: &str) -> Result<String> {
    let d = domain.trim().trim_matches(|c| c == '[' || c == ']');
    if d.is_empty() {
        return Err(ChatmailError::config("domain is empty"));
    }
    if d.parse::<std::net::IpAddr>().is_ok() {
        return Err(ChatmailError::config(
            "use obtain_certificate with a public IP or --auto-ip-cert on install (IP certs use the shortlived profile)",
        ));
    }
    if !d.contains('.') || d == "localhost" || d.ends_with(".local") {
        return Err(ChatmailError::config(format!(
            "invalid domain for ACME: {d}"
        )));
    }
    Ok(d.to_string())
}

/// Returns true if cert is missing or expires within `renew_within_days`.
pub fn cert_needs_renewal(cert_path: &Path, renew_within_days: u32) -> Result<bool> {
    let Ok(pem) = std::fs::read(cert_path) else {
        return Ok(true);
    };
    let cert = X509::from_pem(&pem).map_err(|e| ChatmailError::config(format!("parse cert: {e}")))?;
    let not_after = cert.not_after();
    let not_after_str = not_after.to_string();
    let not_after = openssl::asn1::Asn1Time::from_str(&not_after_str)
        .map_err(|e| ChatmailError::config(format!("parse notAfter: {e}")))?;
    let renew_at = openssl::asn1::Asn1Time::days_from_now(renew_within_days)
        .map_err(|e| ChatmailError::config(format!("renew threshold: {e}")))?;
    Ok(not_after < renew_at)
}

pub(crate) fn write_pem_pair(
    cert_path: &Path,
    key_path: &Path,
    chain_pem: &[u8],
    key_pem: &[u8],
) -> Result<()> {
    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ChatmailError::config(format!("create {}: {e}", parent.display())))?;
    }
    std::fs::write(cert_path, chain_pem)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", cert_path.display())))?;
    std::fs::write(key_path, key_pem)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", key_path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(cert_path, std::fs::Permissions::from_mode(0o640)).ok();
        std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600)).ok();
    }
    Ok(())
}

fn write_lers_certificate(
    cert_path: &Path,
    key_path: &Path,
    certificate: &lers::Certificate,
) -> Result<()> {
    let chain_pem = certificate
        .fullchain_to_pem()
        .map_err(|e| ChatmailError::config(e.to_string()))?;
    let key_pem = certificate
        .private_key_to_pem()
        .map_err(|e| ChatmailError::config(e.to_string()))?;
    write_pem_pair(cert_path, key_path, &chain_pem, &key_pem)
}

fn load_or_generate_account_key(path: &Path) -> Result<PKey<Private>> {
    if path.is_file() {
        let pem = std::fs::read(path)
            .map_err(|e| ChatmailError::config(format!("read {}: {e}", path.display())))?;
        return PKey::private_key_from_pem(&pem)
            .map_err(|e| ChatmailError::config(format!("parse account key: {e}")));
    }
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)
        .map_err(|e| ChatmailError::config(e.to_string()))?;
    let ec = EcKey::generate(&group).map_err(|e| ChatmailError::config(e.to_string()))?;
    PKey::from_ec_key(ec).map_err(|e| ChatmailError::config(e.to_string()))
}

fn save_account_key(path: &Path, key: &PKey<Private>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ChatmailError::config(format!("create {}: {e}", parent.display())))?;
    }
    let pem = key
        .private_key_to_pem_pkcs8()
        .map_err(|e| ChatmailError::config(e.to_string()))?;
    std::fs::write(path, pem)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).ok();
    }
    Ok(())
}

fn map_lers(e: lers::Error) -> ChatmailError {
    ChatmailError::config(e.to_string())
}

pub fn parse_http_listen(addr: &str) -> Result<SocketAddr> {
    addr.parse()
        .map_err(|e| ChatmailError::config(format!("invalid --http-listen {addr:?}: {e}")))
}
