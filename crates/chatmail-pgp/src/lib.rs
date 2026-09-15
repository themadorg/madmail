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
use mail_parser::{Message, MessageParser, MessagePart, MimeHeaders, PartType};

#[derive(Debug, Clone, Default)]
pub struct EnforceOptions {
    pub mail_from: String,
    pub recipients: Vec<String>,
}

/// PGP-only policy gate (Madmail `pgp_verify.EnforceEncryption`).
///
/// Accepts:
/// - Well-formed PGP/MIME (`multipart/encrypted` root with an
///   `application/pgp-encrypted` control part — not a raw substring anywhere)
/// - Secure-Join handshake MIME
/// - Allowed bounce reports (`mailer-daemon` + `multipart/report`)
///
/// Rejects unencrypted mail, including messages that only mention
/// `application/pgp-encrypted` in headers, body text, or MIME parameters.
pub fn enforce_encryption(raw: &[u8], opts: &EnforceOptions) -> Result<()> {
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

    if is_valid_pgp_mime(&msg) {
        return Ok(());
    }

    // Declared as multipart/encrypted but missing the control part.
    if is_multipart_encrypted_root(&msg) {
        return Err(ChatmailError::EncryptionNeeded(
            "invalid PGP/MIME structure".into(),
        ));
    }

    if validate_secure_join_mime(&msg, raw) {
        return Ok(());
    }

    Err(ChatmailError::EncryptionNeeded(
        "Invalid Unencrypted Mail".into(),
    ))
}

/// Root Content-Type is `multipart/encrypted`.
fn is_multipart_encrypted_root(msg: &Message<'_>) -> bool {
    msg.is_content_type("multipart", "encrypted")
}

/// Real PGP/MIME: root is `multipart/encrypted` and some part is
/// `Content-Type: application/pgp-encrypted` (RFC 3156 control part).
fn is_valid_pgp_mime(msg: &Message<'_>) -> bool {
    is_multipart_encrypted_root(msg) && has_pgp_encrypted_control_part(msg)
}

fn has_pgp_encrypted_control_part(msg: &Message<'_>) -> bool {
    for part in &msg.parts {
        if part_is_pgp_encrypted_control(part) {
            return true;
        }
        if let PartType::Message(nested) = &part.body {
            if has_pgp_encrypted_control_part(nested) {
                return true;
            }
        }
    }
    false
}

fn part_is_pgp_encrypted_control(part: &MessagePart<'_>) -> bool {
    part.is_content_type("application", "pgp-encrypted")
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

    // Require multipart/mixed (parsed Content-Type, or Content-Type header only —
    // not a bare body substring).
    let is_mixed = msg.is_content_type("multipart", "mixed")
        || std::str::from_utf8(raw)
            .map(header_content_type_is_multipart_mixed)
            .unwrap_or(false);
    if !is_mixed {
        return false;
    }

    secure_join_body_prefix(msg) || secure_join_body_raw(raw)
}

/// `Content-Type:` header (not body) is multipart/mixed.
fn header_content_type_is_multipart_mixed(text: &str) -> bool {
    let Some((headers, _)) = split_headers_body(text) else {
        return false;
    };
    let mut continued = String::new();
    for line in headers.lines() {
        if line.starts_with([' ', '\t']) {
            continued.push(' ');
            continued.push_str(line.trim());
            continue;
        }
        if !continued.is_empty() {
            if content_type_line_is_multipart_mixed(&continued) {
                return true;
            }
            continued.clear();
        }
        if line.to_ascii_lowercase().starts_with("content-type:") {
            continued = line.to_string();
        }
    }
    if !continued.is_empty() && content_type_line_is_multipart_mixed(&continued) {
        return true;
    }
    false
}

fn content_type_line_is_multipart_mixed(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("content-type:").map(str::trim) else {
        return false;
    };
    rest.starts_with("multipart/mixed")
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
    msg.is_content_type("multipart", "report")
        || msg
            .content_type()
            .map(|c| {
                let t = c.ctype().to_ascii_lowercase();
                t.contains("multipart/report")
                    || (t == "multipart"
                        && c.subtype()
                            .is_some_and(|s| s.eq_ignore_ascii_case("report")))
            })
            .unwrap_or(false)
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

    // --- Substring-bypass regression (correct policy) ---
    //
    // Reject unencrypted mail when `application/pgp-encrypted` appears only in
    // headers, body text, MIME params, etc. Accept only real PGP/MIME, Secure-Join,
    // or bounce.

    const MARKER: &str = "application/pgp-encrypted";

    fn assert_encryption_needed(raw: &[u8], why: &str) {
        assert!(
            matches!(
                enforce_encryption(raw, &EnforceOptions::default()),
                Err(ChatmailError::EncryptionNeeded(_))
            ),
            "{why}"
        );
    }

    fn plain_with(extra_headers: &str, body: &str) -> Vec<u8> {
        format!(
            "From: a@b.test\r\nTo: c@d.test\r\n\
MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\
{extra_headers}\
\r\n\
{body}"
        )
        .into_bytes()
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_body_only() {
        assert_encryption_needed(
            &plain_with("Subject: hi\r\n", &format!("not encrypted\r\n{MARKER}\r\n")),
            "marker only in body",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_subject_only() {
        assert_encryption_needed(
            &plain_with(
                &format!("Subject: {MARKER} subject-only\r\n"),
                "plain body without token\r\n",
            ),
            "marker only in Subject",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_subject_and_body() {
        assert_encryption_needed(
            &plain_with(
                &format!("Subject: {MARKER}\r\n"),
                &format!("hello\r\n{MARKER}\r\n"),
            ),
            "marker in subject+body",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_from_header() {
        let raw = format!(
            "From: \"{MARKER}\" <a@b.test>\r\nTo: c@d.test\r\n\
MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\
Subject: hi\r\n\r\nplain body\r\n"
        );
        assert_encryption_needed(raw.as_bytes(), "marker only in From display-name");
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_to_header() {
        let raw = format!(
            "From: a@b.test\r\nTo: \"{MARKER}\" <c@d.test>\r\n\
MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\
Subject: hi\r\n\r\nplain body\r\n"
        );
        assert_encryption_needed(raw.as_bytes(), "marker only in To");
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_reply_to() {
        assert_encryption_needed(
            &plain_with(
                &format!("Subject: hi\r\nReply-To: {MARKER}@evil.test\r\n"),
                "plain\r\n",
            ),
            "marker only in Reply-To",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_message_id() {
        assert_encryption_needed(
            &plain_with(
                &format!("Subject: hi\r\nMessage-ID: <{MARKER}@b.test>\r\n"),
                "plain\r\n",
            ),
            "marker only in Message-ID",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_user_agent() {
        assert_encryption_needed(
            &plain_with(
                &format!("Subject: hi\r\nUser-Agent: FakeClient ({MARKER})\r\n"),
                "plain\r\n",
            ),
            "marker only in User-Agent",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_x_header() {
        assert_encryption_needed(
            &plain_with(
                &format!("Subject: hi\r\nX-Bypass: {MARKER}\r\n"),
                "plain\r\n",
            ),
            "marker only in X-Bypass",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_as_content_type_name_param() {
        let raw = format!(
            "From: a@b.test\r\nTo: c@d.test\r\nSubject: hi\r\n\
Content-Type: text/plain; name=\"{MARKER}\"\r\n\r\nplain body\r\n"
        );
        assert_encryption_needed(raw.as_bytes(), "marker only as Content-Type name=");
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_as_content_disposition_filename() {
        let raw = format!(
            "From: a@b.test\r\nTo: c@d.test\r\nSubject: hi\r\n\
Content-Type: text/plain\r\n\
Content-Disposition: inline; filename=\"{MARKER}\"\r\n\r\nplain body\r\n"
        );
        assert_encryption_needed(raw.as_bytes(), "marker only as filename=");
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_html_body() {
        let raw = format!(
            "From: a@b.test\r\nTo: c@d.test\r\nSubject: hi\r\n\
Content-Type: text/html; charset=utf-8\r\n\r\n\
<html><body><p>hello {MARKER}</p></body></html>\r\n"
        );
        assert_encryption_needed(raw.as_bytes(), "marker mid HTML body");
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_mid_sentence_body() {
        assert_encryption_needed(
            &plain_with(
                "Subject: hi\r\n",
                &format!("please ignore: prefix {MARKER} suffix, still cleartext\r\n"),
            ),
            "marker mid-sentence in body",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_multipart_alternative() {
        let raw = format!(
            "From: a@b.test\r\nTo: c@d.test\r\nSubject: hi\r\n\
Content-Type: multipart/alternative; boundary=\"alt\"\r\n\r\n\
--alt\r\nContent-Type: text/plain\r\n\r\n{MARKER}\r\n\
--alt\r\nContent-Type: text/html\r\n\r\n<html>hi</html>\r\n\
--alt--\r\n"
        );
        assert_encryption_needed(raw.as_bytes(), "marker inside multipart/alternative");
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_as_mime_boundary() {
        let raw = format!(
            "From: a@b.test\r\nTo: c@d.test\r\nSubject: hi\r\n\
Content-Type: multipart/mixed; boundary=\"{MARKER}\"\r\n\r\n\
--{MARKER}\r\nContent-Type: text/plain\r\n\r\nhello\r\n\
--{MARKER}--\r\n"
        );
        assert_encryption_needed(raw.as_bytes(), "marker used only as MIME boundary");
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_comments_header() {
        assert_encryption_needed(
            &plain_with(
                &format!("Subject: hi\r\nComments: {MARKER}\r\n"),
                "plain\r\n",
            ),
            "marker only in Comments",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_keywords_header() {
        assert_encryption_needed(
            &plain_with(
                &format!("Subject: hi\r\nKeywords: foo, {MARKER}, bar\r\n"),
                "plain\r\n",
            ),
            "marker only in Keywords",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_references() {
        assert_encryption_needed(
            &plain_with(
                &format!("Subject: hi\r\nReferences: <{MARKER}@b.test>\r\n"),
                "plain\r\n",
            ),
            "marker only in References",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_received() {
        assert_encryption_needed(
            &plain_with(
                &format!("Subject: hi\r\nReceived: from x ({MARKER}); date\r\n"),
                "plain\r\n",
            ),
            "marker only in Received",
        );
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_in_headerless_style_body() {
        // Still has minimal routing headers (SMTP always has them); no Content-Type.
        let raw = format!("From: a@b.test\r\nTo: c@d.test\r\nSubject: hi\r\n\r\n{MARKER}\r\n");
        assert_encryption_needed(raw.as_bytes(), "marker-only body without Content-Type");
    }

    #[test]
    fn test_reject_pgp_encrypted_marker_with_noise_prefix_suffix() {
        assert_encryption_needed(
            &plain_with("Subject: hi\r\n", &format!("XXX{MARKER}YYY\r\n")),
            "marker with surrounding noise in body",
        );
    }

    /// Control: near-miss strings without the exact marker must still reject (already does).
    #[test]
    fn test_reject_near_miss_without_exact_marker() {
        for body in [
            "application/pgp-signature\r\n",
            "application/pgp-keys\r\n",
            "multipart/encrypted\r\n",
            "pgp-encrypted\r\n",
            "application/pgp\r\n",
        ] {
            assert_encryption_needed(
                &plain_with("Subject: hi\r\n", body),
                &format!("near-miss body {body:?} must remain rejected"),
            );
        }
    }

    /// Control: real PGP/MIME still accepted (gate must not become total reject).
    #[test]
    fn test_still_accept_real_pgp_mime_after_flaw_cases() {
        assert!(enforce_encryption(PGP_MIME, &EnforceOptions::default()).is_ok());
    }
}
