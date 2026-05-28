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

//! SMTP DATA validation helpers (Madmail `submission.go` / TDD `02-smtp-server.md`).

use chatmail_db::federation_policy_label;
use chatmail_state::policy::{FederationPolicyCache, PolicyMode};
use chatmail_types::{address_domain, address_is_local, ChatmailError, Result};
use chatmail_db::DbPool;

/// Extract a message header value (headers section only).
pub fn header_value(raw: &[u8], name: &str) -> Option<String> {
    let text = std::str::from_utf8(raw).ok()?;
    let want = name.to_ascii_lowercase();
    for line in text.lines() {
        if line.is_empty() {
            break;
        }
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case(&want) {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// Parse `user@domain` from a mailbox header value.
pub fn parse_mailbox_addr(value: &str) -> String {
    let value = value.trim();
    if let Some(lt) = value.find('<') {
        if let Some(gt) = value[lt + 1..].find('>') {
            return value[lt + 1..lt + 1 + gt].trim().to_string();
        }
    }
    value
        .split_whitespace()
        .next()
        .unwrap_or(value)
        .trim_matches(|c| c == '"' || c == '\'')
        .to_string()
}

/// Domain part of an SMTP envelope address.
pub fn envelope_domain(addr: &str) -> Option<String> {
    let addr = addr.trim().trim_start_matches('<').trim_end_matches('>');
    addr.rsplit('@').next().map(|d| d.to_ascii_lowercase())
}

/// Submission checks before PGP gate (TDD: From required, must match MAIL FROM).
pub fn validate_submission_headers(raw: &[u8], mail_from: &str) -> Result<()> {
    let from_hdr = header_value(raw, "From").ok_or_else(|| {
        ChatmailError::protocol("Message does not contain a From header field")
    })?;
    let from_addr = parse_mailbox_addr(&from_hdr);
    if !from_addr.eq_ignore_ascii_case(mail_from) {
        return Err(ChatmailError::protocol(
            "From header does not match envelope sender",
        ));
    }
    Ok(())
}

/// Outbound / submission: reject at RCPT when target domain is blocked (Madmail `remote.go`).
pub async fn check_outbound_rcpt_federation(
    pool: &DbPool,
    policy: &FederationPolicyCache,
    local_domains: &[String],
    rcpt: &str,
) -> Result<()> {
    if address_is_local(rcpt, local_domains) {
        return Ok(());
    }
    let domain = address_domain(rcpt).unwrap_or_default();
    if domain.is_empty() {
        return Ok(());
    }
    let mode = PolicyMode::from_label(&federation_policy_label(pool).await?);
    if policy.check_policy(&domain, mode) {
        Ok(())
    } else {
        Err(ChatmailError::FederationRejected(domain))
    }
}

/// Inbound port 25: federation policy on `MAIL FROM` domain (TDD `02-smtp-server.md`).
pub fn check_inbound_mail_from(
    mail_from: &str,
    policy: &chatmail_state::FederationPolicyCache,
    local_domains: &[String],
    mode: PolicyMode,
) -> Result<()> {
    let domain = envelope_domain(mail_from).unwrap_or_default();
    if domain.is_empty() {
        return Ok(());
    }
    if policy.allows_sender(&domain, local_domains, mode) {
        Ok(())
    } else {
        Err(ChatmailError::FederationRejected(domain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_state::FederationPolicyCache;

    #[test]
    fn parses_from_angle_addr() {
        assert_eq!(
            parse_mailbox_addr("Alice <bob@test.org>"),
            "bob@test.org"
        );
    }

    #[test]
    fn submission_from_must_match_envelope() {
        let raw = b"From: other@test.org\r\nTo: x@test.org\r\nContent-Type: text/plain\r\n\r\nx";
        let err = validate_submission_headers(raw, "sender@test.org").unwrap_err();
        assert!(matches!(err, ChatmailError::Protocol(_)));
    }

    #[test]
    fn federation_rejects_blocked_domain() {
        let cache = FederationPolicyCache::new();
        cache.add_exception("evil.test");
        let err = check_inbound_mail_from(
            "sender@evil.test",
            &cache,
            &["local.test".into()],
            PolicyMode::Accept,
        )
        .unwrap_err();
        assert!(matches!(err, ChatmailError::FederationRejected(_)));
    }
}
