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

//! TLS certificate issuance for madmail-v2 (Madmail-compatible paths).
//!
//! - **self_signed** — local/IP testing (`/var/lib/<binary>/certs/`)
//! - **autocert** — Let's Encrypt via HTTP-01 (`madmail certificate get|regenerate`)
//! - **IP autocert** — Let's Encrypt short-lived profile (~6 days) for public IPs

mod acme_common;
mod http01;
mod obtain;
mod obtain_ip;
mod self_signed;
mod status;

pub use acme_common::resolve_domain_to_public_ip;
pub use obtain::{
    cert_needs_renewal, obtain_certificate, parse_http_listen, ObtainOptions,
    DNS_CERT_RENEW_WITHIN_DAYS,
};
pub use obtain_ip::{
    is_public_ip, obtain_ip_certificate, parse_public_ip, IP_CERT_RENEW_WITHIN_DAYS,
    LETS_ENCRYPT_SHORTLIVED_PROFILE,
};
pub use self_signed::generate_self_signed;
pub use status::{read_certificate_info, CertIssuerKind, CertificateInfo};

/// Whether `domain` is suitable for Let's Encrypt DNS identifiers.
pub fn is_valid_dns_domain(domain: &str) -> bool {
    let d = domain.trim().trim_matches(|c| c == '[' || c == ']');
    if d.is_empty() || !d.contains('.') {
        return false;
    }
    if d == "localhost" || d.ends_with(".local") || d == "example.org" {
        return false;
    }
    d.parse::<std::net::IpAddr>().is_err()
}

/// Whether `domain` is a public IP eligible for Let's Encrypt IP certificates.
pub fn is_valid_ip_for_acme(domain: &str) -> bool {
    parse_public_ip(domain).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dns_domain_validation() {
        assert!(!is_valid_dns_domain("[1.1.1.1]"));
        assert!(!is_valid_dns_domain("localhost"));
    }

    #[test]
    fn ip_acme_validation() {
        assert!(is_valid_ip_for_acme("1.1.1.1"));
        assert!(is_valid_ip_for_acme("[1.1.1.1]"));
        assert!(!is_valid_ip_for_acme("192.168.1.1"));
    }

    #[test]
    fn self_signed_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let cert = dir.path().join("fullchain.pem");
        let key = dir.path().join("privkey.pem");
        generate_self_signed("[1.2.3.4]", "[1.2.3.4]", "1.2.3.4", &cert, &key).unwrap();
        assert!(cert.is_file());
        assert!(key.is_file());
    }
}
