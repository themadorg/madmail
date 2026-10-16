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

//! Local delivery-status report for a failed outbound message.
//!
//! Delta Chat matches `multipart/report` (`report-type=delivery-status`)
//! against the original `Message-ID` and shows the text part on that message.
//! The PGP gate already accepts `mailer-daemon` plus `multipart/report`.

use chatmail_auth::normalize_username;
use chatmail_pgp::{enforce_encryption, EnforceOptions};
use chatmail_types::address_domain;
use tracing::warn;
use uuid::Uuid;

use crate::router::DeliveryContext;

/// Why the queued message will not be tried again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Permanent,
    Expired,
    GaveUp,
}

pub struct FailureNotice<'a> {
    pub mail_from: &'a str,
    pub rcpt_to: &'a str,
    pub reason: &'a str,
    pub original: &'a [u8],
    pub kind: FailureKind,
}

/// Write a failure report into the local sender's inbox.
///
/// Remote senders are left alone, so a report is never federated onward.
/// A report whose sender is already `mailer-daemon` is dropped.
pub async fn deliver_failure_notice(ctx: &DeliveryContext, notice: FailureNotice<'_>) {
    let Ok(sender) = normalize_username(notice.mail_from) else {
        return;
    };
    if !ctx.is_local(&sender) || is_mailer_daemon(&sender) {
        return;
    }
    let Some(domain) = address_domain(&sender) else {
        return;
    };
    let Some(raw) = build_failure_report(&domain, &sender, &notice) else {
        return;
    };
    let daemon = format!("mailer-daemon@{domain}");
    if enforce_encryption(
        &raw,
        &EnforceOptions {
            mail_from: daemon.clone(),
            recipients: vec![sender.clone()],
        },
    )
    .is_err()
    {
        warn!(sender = %sender, "failure report rejected by the encryption gate");
        return;
    }
    if let Err(e) = ctx.state.check_message_size(raw.len()) {
        warn!(sender = %sender, error = %e, "failure report exceeds the message size");
        return;
    }
    if let Err(e) = ctx
        .state
        .check_quota(&sender, raw.len() as u64, "federation.dsn")
    {
        warn!(sender = %sender, error = %e, "failure report skipped, quota full");
        return;
    }

    let msg_id = Uuid::new_v4().to_string();
    let outcome = match chatmail_storage::deliver_local_messages(
        &ctx.state.mailbox_store,
        &[(sender.clone(), msg_id.clone())],
        &raw,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(e) => {
            warn!(sender = %sender, error = %e, "failure report was not stored");
            return;
        }
    };
    if outcome.delivered.is_empty() {
        warn!(sender = %sender, "failure report was not stored");
        return;
    }
    ctx.state.quota.record_write(&sender, raw.len() as u64);
    ctx.state.events.notify_new_message(&sender, &msg_id);
    ctx.state
        .notify_inbound_push(&ctx.pool, &daemon, &sender)
        .await;
}

pub fn build_failure_report(
    domain: &str,
    sender: &str,
    notice: &FailureNotice<'_>,
) -> Option<Vec<u8>> {
    if domain.is_empty() || domain_chars_invalid(domain) {
        return None;
    }
    let recipient = header_safe(notice.rcpt_to);
    if recipient.is_empty() {
        return None;
    }
    let reason = bounded(notice.reason.trim(), 2000);
    let explanation = match notice.kind {
        FailureKind::Permanent => format!("Your message to {recipient} was not delivered."),
        FailureKind::Expired => {
            format!("Your message to {recipient} expired before it could be delivered.")
        }
        FailureKind::GaveUp => {
            format!("Your message to {recipient} could not be delivered after repeated attempts.")
        }
    };
    let mut text = explanation;
    if !reason.is_empty() {
        text.push_str("\r\n\r\n");
        text.push_str(reason);
    }
    let status = match notice.kind {
        FailureKind::Permanent => "5.7.1",
        FailureKind::Expired | FailureKind::GaveUp => "5.4.7",
    };
    let diagnostic = header_safe(reason);
    let diagnostic = if diagnostic.is_empty() {
        "delivery failed".to_string()
    } else {
        diagnostic
    };
    let boundary = format!("dsn{}", Uuid::new_v4().simple());
    if text.contains(&boundary) || diagnostic.contains(&boundary) {
        return None;
    }
    let headers = original_headers(notice.original);
    let from = format!("mailer-daemon@{domain}");
    let msg_id = format!("<dsn-{}@{domain}>", Uuid::new_v4().simple());
    let raw = format!(
        "From: Mail Delivery System <{from}>\r\n\
         To: <{sender}>\r\n\
         Message-ID: {msg_id}\r\n\
         Auto-Submitted: auto-replied\r\n\
         Subject: Delivery failed\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/report; report-type=delivery-status;\r\n\
         \tboundary=\"{boundary}\"\r\n\
         \r\n\
         --{boundary}\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: 8bit\r\n\
         \r\n\
         {text}\r\n\
         \r\n\
         --{boundary}\r\n\
         Content-Type: message/delivery-status\r\n\
         \r\n\
         Reporting-MTA: dns; {domain}\r\n\
         \r\n\
         Final-Recipient: rfc822; {recipient}\r\n\
         Action: failed\r\n\
         Status: {status}\r\n\
         Diagnostic-Code: smtp; {diagnostic}\r\n\
         \r\n\
         --{boundary}\r\n\
         Content-Type: text/rfc822-headers\r\n\
         \r\n\
         {headers}\r\n\
         --{boundary}--\r\n"
    );
    Some(raw.into_bytes())
}

fn bounded(value: &str, max: usize) -> &str {
    if value.len() <= max {
        return value;
    }
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn is_mailer_daemon(addr: &str) -> bool {
    addr.split('@')
        .next()
        .is_some_and(|local| local.eq_ignore_ascii_case("mailer-daemon"))
}

fn domain_chars_invalid(domain: &str) -> bool {
    domain
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '@' | '<' | '>' | '\r' | '\n'))
}

/// One header line. Newlines and non-ASCII bytes cannot add another field.
fn header_safe(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        if out.len() >= 400 {
            break;
        }
        if c.is_ascii_graphic() || c == ' ' {
            out.push(c);
        } else if matches!(c, '\n' | '\r' | '\t') && !out.ends_with(' ') {
            out.push(' ');
        }
    }
    out.trim().to_string()
}

fn original_headers(raw: &[u8]) -> String {
    let sep = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .or_else(|| raw.windows(2).position(|w| w == b"\n\n"));
    let end = sep.unwrap_or(raw.len()).min(8192);
    let mut headers = raw[..end].to_vec();
    while headers.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
        headers.pop();
    }
    if !headers.is_empty() {
        headers.extend_from_slice(b"\r\n");
    }
    String::from_utf8_lossy(&headers).replace(['\r', '\0'], "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_pgp::{enforce_encryption, EnforceOptions};
    use chatmail_state::AppState;
    use std::sync::Arc;

    fn sample_original() -> &'static [u8] {
        b"From: a@example.org\r\nTo: b@other.org\r\nMessage-ID: <abc@example.org>\r\nSubject: hi\r\n\r\nbody"
    }

    #[test]
    fn report_is_a_delivery_status_delta_chat_can_match() {
        let notice = FailureNotice {
            mail_from: "a@example.org",
            rcpt_to: "b@other.org\r\nAction: delivered",
            reason: "federation transport blocked by policy\r\nAction: delivered",
            original: sample_original(),
            kind: FailureKind::Permanent,
        };
        let raw = build_failure_report("example.org", "a@example.org", &notice).unwrap();
        let text = String::from_utf8(raw.clone()).unwrap();
        assert!(text.contains("multipart/report"));
        assert!(text.contains("report-type=delivery-status"));
        let status = text.split("message/delivery-status").nth(1).unwrap();
        assert!(status.lines().any(|line| line == "Action: failed"));
        assert!(!status
            .lines()
            .any(|line| line.eq_ignore_ascii_case("Action: delivered")));
        assert!(text.contains("Status: 5.7.1"));
        assert!(text.contains("Message-ID: <abc@example.org>"));
        assert!(text.contains("federation transport blocked by policy"));
        assert!(text.contains("mailer-daemon@example.org"));
        assert!(enforce_encryption(
            &raw,
            &EnforceOptions {
                mail_from: "mailer-daemon@example.org".into(),
                recipients: vec!["a@example.org".into()],
            },
        )
        .is_ok());
    }

    #[test]
    fn expiry_uses_the_timeout_status() {
        let notice = FailureNotice {
            mail_from: "a@example.org",
            rcpt_to: "b@other.org",
            reason: "connection timed out",
            original: sample_original(),
            kind: FailureKind::Expired,
        };
        let raw = build_failure_report("example.org", "a@example.org", &notice).unwrap();
        let text = String::from_utf8(raw).unwrap();
        assert!(text.contains("expired before it could be delivered"));
        assert!(text.contains("Status: 5.4.7"));
        assert!(text.contains("connection timed out"));
    }

    #[tokio::test]
    async fn local_sender_gets_one_report_and_a_daemon_does_not() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path(), pool.clone()));
        let ctx = DeliveryContext {
            pool,
            state: Arc::clone(&state),
            primary_domain: "example.org".into(),
            local_domains: vec!["example.org".into()],
            dkim: None,
        };
        deliver_failure_notice(
            &ctx,
            FailureNotice {
                mail_from: "A@Example.org",
                rcpt_to: "b@other.org",
                reason: "federation transport blocked by policy",
                original: sample_original(),
                kind: FailureKind::Permanent,
            },
        )
        .await;
        let new_dir = state.mailbox_store.maildir_for_user("a@example.org").new;
        let files: Vec<_> = std::fs::read_dir(&new_dir).unwrap().collect();
        assert_eq!(files.len(), 1);
        let raw = std::fs::read(files[0].as_ref().unwrap().path()).unwrap();
        let text = String::from_utf8(raw).unwrap();
        assert!(text.contains("Message-ID: <abc@example.org>"));
        assert!(text.contains("mailer-daemon@example.org"));

        deliver_failure_notice(
            &ctx,
            FailureNotice {
                mail_from: "mailer-daemon@example.org",
                rcpt_to: "a@example.org",
                reason: "nested",
                original: b"From: mailer-daemon@example.org\r\n\r\n",
                kind: FailureKind::Permanent,
            },
        )
        .await;
        deliver_failure_notice(
            &ctx,
            FailureNotice {
                mail_from: "a@remote.org",
                rcpt_to: "b@other.org",
                reason: "remote sender",
                original: sample_original(),
                kind: FailureKind::GaveUp,
            },
        )
        .await;
        let files: Vec<_> = std::fs::read_dir(&new_dir).unwrap().collect();
        assert_eq!(files.len(), 1);
        assert!(!state
            .mailbox_store
            .maildir_for_user("a@remote.org")
            .root
            .exists());
    }
}
