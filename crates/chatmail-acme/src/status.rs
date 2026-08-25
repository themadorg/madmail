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

//! Inspect PEM certificates for `madmail certificate status`.

use std::path::Path;

use chatmail_types::{ChatmailError, Result};
use openssl::x509::X509;

/// How the certificate appears to have been issued (from PEM metadata).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertIssuerKind {
    LetsEncrypt,
    SelfSigned,
    Other,
}

/// Parsed fields from an on-disk PEM certificate.
#[derive(Debug, Clone)]
pub struct CertificateInfo {
    pub issuer: String,
    pub subject: String,
    pub not_before: String,
    pub not_after: String,
    pub days_remaining: i64,
    pub subject_alt_names: Vec<String>,
    pub issuer_kind: CertIssuerKind,
    /// Public-key algorithm (`ecdsa-p256`, `rsa-2048`, …).
    pub key_algorithm: String,
}

pub fn read_certificate_info(cert_path: &Path) -> Result<Option<CertificateInfo>> {
    let Ok(pem) = std::fs::read(cert_path) else {
        return Ok(None);
    };
    let cert =
        X509::from_pem(&pem).map_err(|e| ChatmailError::config(format!("parse cert: {e}")))?;
    let issuer = dn_to_string(cert.issuer_name());
    let subject = dn_to_string(cert.subject_name());
    let not_before = cert.not_before().to_string();
    let not_after = cert.not_after().to_string();
    let days_remaining = days_until(cert.not_after())?;
    let subject_alt_names = extract_subject_alt_names(&cert);
    let issuer_kind = classify_issuer(&issuer, &subject);
    let key_algorithm = describe_key_algorithm(&cert);
    Ok(Some(CertificateInfo {
        issuer,
        subject,
        not_before,
        not_after,
        days_remaining,
        subject_alt_names,
        issuer_kind,
        key_algorithm,
    }))
}

fn days_until(not_after: &openssl::asn1::Asn1TimeRef) -> Result<i64> {
    let now = openssl::asn1::Asn1Time::days_from_now(0)
        .map_err(|e| ChatmailError::config(format!("current time: {e}")))?;
    let diff = now
        .as_ref()
        .diff(not_after)
        .map_err(|e| ChatmailError::config(e.to_string()))?;
    let total_secs = diff.days.unsigned_abs() as i64 * 86_400 + diff.secs.unsigned_abs() as i64;
    Ok((total_secs + 86_399) / 86_400)
}

fn dn_to_string(name: &openssl::x509::X509NameRef) -> String {
    name.entries()
        .map(|e| {
            let key = e.object().nid().short_name().unwrap_or("?");
            let val = e
                .data()
                .as_utf8()
                .map(|s| s.to_string())
                .unwrap_or_else(|_| String::new());
            format!("{key}={val}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn ip_to_string(bytes: &[u8]) -> Option<String> {
    match bytes.len() {
        4 => Some(std::net::IpAddr::from([bytes[0], bytes[1], bytes[2], bytes[3]]).to_string()),
        16 => {
            let mut octets = [0u8; 16];
            octets.copy_from_slice(bytes);
            Some(std::net::IpAddr::from(octets).to_string())
        }
        _ => None,
    }
}

fn describe_key_algorithm(cert: &X509) -> String {
    let Ok(pkey) = cert.public_key() else {
        return "unknown".into();
    };
    match pkey.id() {
        openssl::pkey::Id::RSA => format!("rsa-{}", pkey.bits()),
        openssl::pkey::Id::EC => match pkey.ec_key() {
            Ok(ec) => match ec.group().curve_name() {
                Some(openssl::nid::Nid::X9_62_PRIME256V1) => "ecdsa-p256".into(),
                Some(openssl::nid::Nid::SECP384R1) => "ecdsa-p384".into(),
                Some(openssl::nid::Nid::SECP521R1) => "ecdsa-p521".into(),
                Some(nid) => format!(
                    "ecdsa-{}",
                    nid.short_name().unwrap_or("unknown").to_ascii_lowercase()
                ),
                None => format!("ecdsa-{}", pkey.bits()),
            },
            Err(_) => "ecdsa".into(),
        },
        openssl::pkey::Id::ED25519 => "ed25519".into(),
        other => format!("id-{}", other.as_raw()),
    }
}

fn classify_issuer(issuer: &str, subject: &str) -> CertIssuerKind {
    let issuer_l = issuer.to_ascii_lowercase();
    if issuer_l.contains("let's encrypt") || issuer_l.contains("lets encrypt") {
        return CertIssuerKind::LetsEncrypt;
    }
    if subject == issuer || issuer_l.contains("rcgen") {
        return CertIssuerKind::SelfSigned;
    }
    CertIssuerKind::Other
}

fn extract_subject_alt_names(cert: &X509) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(san) = cert.subject_alt_names() {
        for name in san.iter() {
            if let Some(dns) = name.dnsname() {
                names.push(dns.to_string());
            } else if let Some(ip) = name.ipaddress() {
                if let Some(addr) = ip_to_string(ip) {
                    names.push(addr);
                }
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::self_signed::generate_self_signed;

    #[test]
    fn reads_self_signed_info() {
        let dir = tempfile::tempdir().unwrap();
        let cert = dir.path().join("fullchain.pem");
        let key = dir.path().join("privkey.pem");
        generate_self_signed("[1.2.3.4]", "1.2.3.4", "1.2.3.4", &cert, &key).unwrap();
        let info = read_certificate_info(&cert).unwrap().unwrap();
        assert_eq!(info.issuer_kind, CertIssuerKind::SelfSigned);
        assert_eq!(info.key_algorithm, "ecdsa-p256");
        assert!(info.days_remaining > 0);
        let key_pem = std::fs::read_to_string(&key).unwrap();
        assert!(
            key_pem.contains("-----BEGIN PRIVATE KEY-----"),
            "self-signed key should be PKCS#8: {key_pem}"
        );
        assert!(
            !key_pem.contains("BEGIN EC PRIVATE KEY"),
            "self-signed key must not be SEC1 EC: {key_pem}"
        );
        assert!(
            !key_pem.contains("BEGIN RSA PRIVATE KEY"),
            "self-signed key must not be RSA PKCS#1"
        );
    }
}
