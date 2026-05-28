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

//! Mail domain handling for IP-literal (`user@[1.2.3.4]`) and DNS names (`user@a.com`).
//!
//! Mirrors Madmail `auth.WrapIP`, `framework/address`, and install `local_domains` wiring.

use std::collections::HashSet;

/// True if `s` looks like an IPv4 address (no port).
pub fn is_ipv4_literal(s: &str) -> bool {
    let s = s.trim().trim_matches(|c| c == '[' || c == ']');
    let parts: Vec<_> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| p.parse::<u8>().is_ok())
}

/// Canonical domain for config: bare IPs become `[1.2.3.4]` (RFC 5321 address-literal).
pub fn wrap_ip_domain(domain: &str) -> String {
    let trimmed = domain.trim();
    let bare = trimmed.trim_matches(|c| c == '[' || c == ']');
    if is_ipv4_literal(bare) {
        format!("[{bare}]")
    } else {
        trimmed.to_string()
    }
}

/// Accepted forms for matching: `example.org`, `[1.2.3.4]`, and bare `1.2.3.4`.
pub fn domain_forms(domain: &str) -> Vec<String> {
    let lower = wrap_ip_domain(domain).to_ascii_lowercase();
    let mut forms = HashSet::new();
    forms.insert(lower.clone());
    let stripped = lower.trim_matches(|c| c == '[' || c == ']');
    if stripped != lower {
        forms.insert(stripped.to_string());
    } else if is_ipv4_literal(stripped) {
        forms.insert(format!("[{stripped}]"));
    }
    forms.into_iter().collect()
}

/// Build the set of domains this server accepts for local delivery (from `maddy.conf`).
pub fn build_local_domains(primary_domain: &str, local_domains: Option<&str>) -> Vec<String> {
    let mut all = HashSet::new();
    for form in domain_forms(primary_domain) {
        all.insert(form);
    }
    if let Some(list) = local_domains {
        for token in list.split_whitespace() {
            if token.is_empty() {
                continue;
            }
            for form in domain_forms(token) {
                all.insert(form);
            }
        }
    }
    let mut v: Vec<_> = all.into_iter().collect();
    v.sort();
    v
}

/// Domain part of `user@domain`, normalized (IPs wrapped in brackets).
pub fn address_domain(addr: &str) -> Option<String> {
    let addr = addr.trim().trim_start_matches('<').trim_end_matches('>');
    let (_, domain) = addr.rsplit_once('@')?;
    Some(wrap_ip_domain(domain).to_ascii_lowercase())
}

/// Whether `addr` is a local recipient on this server.
pub fn address_is_local(addr: &str, accepted_domains: &[String]) -> bool {
    let Some(rcpt_domain) = address_domain(addr) else {
        return false;
    };
    accepted_domains.iter().any(|allowed| {
        domain_forms(allowed)
            .iter()
            .any(|form| form.eq_ignore_ascii_case(&rcpt_domain))
    })
}

/// JIT login restriction: username must be `local@expected` (Madmail `ValidateLoginDomain`).
pub fn validate_login_domain(username: &str, expected_domain: &str) -> Result<(), String> {
    if expected_domain.is_empty() {
        return Ok(());
    }
    if username.contains('%') {
        return Err("invalid username: contains URL-encoded characters".into());
    }
    let Some((local, domain)) = username.rsplit_once('@') else {
        return Err("invalid username format: expected localpart@domain".into());
    };
    if local.is_empty() {
        return Err("invalid username: empty localpart".into());
    }
    let domain = wrap_ip_domain(domain);
    let expected = wrap_ip_domain(expected_domain);
    if domain.eq_ignore_ascii_case(&expected) {
        Ok(())
    } else {
        Err(format!("invalid login domain: expected @{expected}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_bare_ipv4() {
        assert_eq!(wrap_ip_domain("1.2.3.4"), "[1.2.3.4]");
        assert_eq!(wrap_ip_domain("[1.2.3.4]"), "[1.2.3.4]");
        assert_eq!(wrap_ip_domain("mail.example.org"), "mail.example.org");
    }

    #[test]
    fn local_rcpt_accepts_bracket_and_bare_ip() {
        let accepted = build_local_domains("[1.1.1.1]", None);
        assert!(address_is_local("alice@[1.1.1.1]", &accepted));
        assert!(address_is_local("alice@1.1.1.1", &accepted));
        assert!(!address_is_local("alice@other.com", &accepted));
    }

    #[test]
    fn local_domains_list_accepts_ip_and_dns() {
        let accepted = build_local_domains("a.com", Some("a.com [1.1.1.1] 1.1.1.1"));
        assert!(address_is_local("u@a.com", &accepted));
        assert!(address_is_local("u@[1.1.1.1]", &accepted));
        assert!(address_is_local("u@1.1.1.1", &accepted));
        assert!(!address_is_local("u@b.com", &accepted));
    }

    #[test]
    fn validate_login_domain_ip() {
        assert!(validate_login_domain("x@[1.1.1.1]", "[1.1.1.1]").is_ok());
        assert!(validate_login_domain("x@1.1.1.1", "[1.1.1.1]").is_ok());
        assert!(validate_login_domain("x@wrong.com", "[1.1.1.1]").is_err());
    }
}
