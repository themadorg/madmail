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

use chatmail_types::{ChatmailError, Result};
use mail_parser::{Message, MessageParser, MimeHeaders};

#[derive(Debug, Clone, Default)]
pub struct EnforceOptions {
    pub mail_from: String,
    pub recipients: Vec<String>,
}

/// PGP-only policy gate (Madmail `pgp_verify.EnforceEncryption`).
pub fn enforce_encryption(raw: &[u8], opts: &EnforceOptions) -> Result<()> {
    if raw
        .windows(b"application/pgp-encrypted".len())
        .any(|w| w == b"application/pgp-encrypted")
    {
        return Ok(());
    }

    if is_allowed_bounce_raw(raw, &opts.mail_from) {
        return Ok(());
    }

    let Some(msg) = MessageParser::default().parse(raw) else {
        return Err(ChatmailError::EncryptionNeeded(
            "unparseable message".into(),
        ));
    };

    if is_allowed_bounce(&msg, &opts.mail_from) {
        return Ok(());
    }

    let ct = msg
        .content_type()
        .map(|c| c.ctype().to_ascii_lowercase())
        .unwrap_or_default();
    let raw_lc = std::str::from_utf8(raw)
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    if ct.contains("multipart/encrypted") || raw_lc.contains("multipart/encrypted") {
        return Err(ChatmailError::EncryptionNeeded(
            "invalid PGP/MIME structure".into(),
        ));
    }

    if ct.contains("multipart/mixed") || raw_lc.contains("multipart/mixed") {
        if validate_secure_join_mime(&msg, raw) {
            return Ok(());
        }
        return Err(ChatmailError::EncryptionNeeded(
            "Invalid Unencrypted Mail".into(),
        ));
    }

    Err(ChatmailError::EncryptionNeeded(
        "Invalid Unencrypted Mail".into(),
    ))
}

/// Delta Chat Secure-Join handshake (Madmail `isSecureJoinHeader` + `streamValidateSecureJoinMIME`).
fn validate_secure_join_mime(msg: &Message<'_>, raw: &[u8]) -> bool {
    let step = secure_join_step(msg).or_else(|| secure_join_step_raw(raw));
    let Some(step) = step else {
        return false;
    };
    if !step.starts_with("vc-") && !step.starts_with("vg-") {
        return false;
    }
    secure_join_body_prefix(msg) || secure_join_body_raw(raw)
}

fn secure_join_step_raw(raw: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(raw).ok()?;
    let (headers, _) = split_headers_body(text)?;
    for line in headers.lines() {
        let lower = line.trim().to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("secure-join:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn secure_join_body_raw(raw: &[u8]) -> bool {
    let text = match std::str::from_utf8(raw) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let (_, body) = match split_headers_body(text) {
        Some(x) => x,
        None => return false,
    };
    let head: String = body
        .chars()
        .take(128)
        .collect::<String>()
        .trim_start()
        .to_ascii_lowercase();
    head.contains("secure-join:")
}

fn split_headers_body(text: &str) -> Option<(&str, &str)> {
    text.find("\r\n\r\n")
        .map(|i| (&text[..i], &text[i + 4..]))
        .or_else(|| text.find("\n\n").map(|i| (&text[..i], &text[i + 2..])))
}

fn secure_join_step(msg: &Message<'_>) -> Option<String> {
    msg.headers()
        .iter()
        .find(|h| h.name().eq_ignore_ascii_case("Secure-Join"))
        .and_then(|h| h.value().as_text())
        .map(|s| s.trim().to_ascii_lowercase())
}

fn secure_join_body_prefix(msg: &Message<'_>) -> bool {
    for idx in 0..8 {
        let Some(text) = msg.body_text(idx) else {
            continue;
        };
        let head: String = text
            .chars()
            .take(64)
            .collect::<String>()
            .trim_start()
            .to_ascii_lowercase();
        if head.starts_with("secure-join:") {
            return true;
        }
    }
    false
}

fn is_allowed_bounce_raw(raw: &[u8], mail_from: &str) -> bool {
    if !mail_from.to_ascii_lowercase().contains("mailer-daemon") {
        return false;
    }
    std::str::from_utf8(raw)
        .map(|s| s.to_ascii_lowercase().contains("multipart/report"))
        .unwrap_or(false)
}

fn is_allowed_bounce(msg: &Message<'_>, mail_from: &str) -> bool {
    if !mail_from.to_ascii_lowercase().contains("mailer-daemon") {
        return false;
    }
    let ct = msg
        .content_type()
        .map(|c| c.ctype().to_ascii_lowercase())
        .unwrap_or_default();
    ct.contains("multipart/report")
}

/// Build an unencrypted `vc-request` like relay-ping / Delta Chat Bob step 2.
#[cfg(test)]
pub fn build_vc_request_raw(from: &str, to: &str, invite_number: &str) -> String {
    let boundary = format!("securejoin-{}", invite_number);
    let domain = from.rsplit('@').next().unwrap_or("test");
    let msg_id = format!("<sj-{invite_number}@{domain}>");
    format!(
        "From: <{from}>\r\n\
To: <{to}>\r\n\
Date: Tue, 6 Jan 2026 08:20:47 +0000\r\n\
Message-ID: {msg_id}\r\n\
Subject: [...]\r\n\
Chat-Version: 1.0\r\n\
Secure-Join: vc-request\r\n\
Secure-Join-Invitenumber: {invite_number}\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\
\r\n\
--{boundary}\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
secure-join: vc-request\r\n\
\r\n\
--{boundary}--\r\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PGP_MIME: &[u8] = b"From: a@b.test\r\nTo: c@d.test\r\nContent-Type: multipart/encrypted; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n--b--\r\n";

    /// P4-UT01 / TDD §1 PGP reject plaintext
    #[test]
    fn p4_ut01_test_reject_plaintext() {
        let raw = b"From: a@b.test\r\nTo: c@d.test\r\nSubject: hi\r\nContent-Type: text/plain\r\n\r\nhello";
        assert!(matches!(
            enforce_encryption(raw, &EnforceOptions::default()),
            Err(ChatmailError::EncryptionNeeded(_))
        ));
    }

    /// P4-UT02 / TDD §1 PGP accept multipart/encrypted
    #[test]
    fn p4_ut02_test_accept_pgp_mime() {
        assert!(enforce_encryption(PGP_MIME, &EnforceOptions::default()).is_ok());
    }

    /// TDD 16-testing: Secure-Join multipart/mixed handshake bypasses encryption check.
    #[test]
    fn test_secure_join_vc_request_multipart_accepted() {
        let raw = build_vc_request_raw("bob@test", "alice@test", "invite-token-123");
        assert!(enforce_encryption(raw.as_bytes(), &EnforceOptions::default()).is_ok());
    }

    /// Header-only plaintext is not a valid Secure-Join MIME (Madmail parity).
    #[test]
    fn test_secure_join_header_only_plaintext_rejected() {
        let raw = b"From: a@b.test\r\nTo: c@d.test\r\nSecure-Join: vc-request\r\nContent-Type: text/plain\r\n\r\nsetup";
        assert!(enforce_encryption(raw, &EnforceOptions::default()).is_err());
    }

    /// multipart/mixed with wrong body line is rejected.
    #[test]
    fn test_secure_join_bad_body_rejected() {
        let raw = b"From: a@b.test\r\nTo: c@d.test\r\nSecure-Join: vc-request\r\nContent-Type: multipart/mixed; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: text/plain\r\n\r\nnot-secure-join\r\n--b--\r\n";
        assert!(enforce_encryption(raw, &EnforceOptions::default()).is_err());
    }

    /// TDD 12-security: mailer-daemon multipart/report bounces allowed.
    #[test]
    fn test_mailer_daemon_bounce_allowed() {
        let raw = b"From: mailer-daemon@b.test\r\nTo: c@d.test\r\nContent-Type: multipart/report\r\n\r\nreport";
        let opts = EnforceOptions {
            mail_from: "mailer-daemon@b.test".into(),
            recipients: vec![],
        };
        assert!(enforce_encryption(raw, &opts).is_ok());
    }

    /// Invalid multipart/encrypted without pgp-encrypted part still fails when only ctype is set.
    #[test]
    fn test_multipart_encrypted_without_pgp_part_rejected() {
        let raw = b"From: a@b.test\r\nTo: c@d.test\r\nContent-Type: multipart/encrypted; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain\r\n\r\nnope\r\n--x--\r\n";
        assert!(enforce_encryption(raw, &EnforceOptions::default()).is_err());
    }
}
