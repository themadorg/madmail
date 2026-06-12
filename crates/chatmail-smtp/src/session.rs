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

use std::sync::Arc;

use base64::Engine;
use chatmail_auth::{normalize_username, AuthContext};
use chatmail_config::CredentialPolicy;
use chatmail_db::DbPool;
use chatmail_delivery::DeliveryContext;
use chatmail_pgp::{enforce_encryption, EnforceOptions};
use chatmail_state::AppState;
use chatmail_storage::deliver_local_messages;
use chatmail_types::{ChatmailError, Result};
use rustls::ServerConfig;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

use crate::data_limit::{parse_smtp_size_parameter, read_smtp_data_limited};
use crate::protocol::{
    check_inbound_mail_from, check_outbound_rcpt_federation, validate_submission_headers,
};

#[derive(Clone)]
pub struct SmtpSessionConfig {
    pub hostname: String,
    pub primary_domain: String,
    pub local_domains: Vec<String>,
    pub jit_domain: Option<String>,
    pub credential_policy: CredentialPolicy,
    /// Submission (587/465): AUTH required, PGP + From/envelope checks.
    pub require_auth: bool,
    /// Prometheus label (`smtp` / `submission`), matches Madmail endpoint name.
    pub module: &'static str,
    /// TLS upgrade on cleartext submission (port 587); not used on implicit-TLS :465.
    pub starttls_config: Option<Arc<ServerConfig>>,
}

pub struct SmtpSession {
    pub ctx: Arc<AppState>,
    pub pool: DbPool,
    pub cfg: SmtpSessionConfig,
    authenticated_user: Option<String>,
    mail_from: String,
    rcpt_to: Vec<String>,
    seen_ehlo: bool,
}

impl SmtpSession {
    pub fn new(ctx: Arc<AppState>, pool: DbPool, cfg: SmtpSessionConfig) -> Self {
        Self {
            ctx,
            pool,
            cfg,
            authenticated_user: None,
            mail_from: String::new(),
            rcpt_to: Vec::new(),
            seen_ehlo: false,
        }
    }

    pub async fn handle_connection(&mut self, stream: TcpStream) -> Result<()> {
        if self.cfg.starttls_config.is_some() {
            self.serve_with_starttls_upgrade(stream).await
        } else {
            let (reader, writer) = tokio::io::split(stream);
            self.serve(reader, writer, false).await
        }
    }

    pub async fn handle_tls_connection(&mut self, stream: TlsStream<TcpStream>) -> Result<()> {
        let (reader, writer) = tokio::io::split(stream);
        self.serve(reader, writer, true).await
    }

    async fn serve_with_starttls_upgrade(&mut self, stream: TcpStream) -> Result<()> {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader);

        writer
            .write_all(format!("220 {} ESMTP madmail-v2\r\n", self.cfg.hostname).as_bytes())
            .await?;

        loop {
            let mut line = String::new();
            if lines.read_line(&mut line).await? == 0 {
                break;
            }
            let line = line.trim_end().to_string();
            if line.is_empty() {
                continue;
            }
            let cmd = line
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            match cmd.as_str() {
                "EHLO" | "HELO" => {
                    self.seen_ehlo = true;
                    writer.write_all(self.format_ehlo(false).as_bytes()).await?;
                }
                "STARTTLS" => {
                    let Some(cfg) = self.cfg.starttls_config.clone() else {
                        writer
                            .write_all(b"502 5.5.1 STARTTLS not available\r\n")
                            .await?;
                        continue;
                    };
                    writer
                        .write_all(b"220 2.0.0 Ready to start TLS\r\n")
                        .await?;
                    writer.flush().await?;
                    let reader = lines.into_inner();
                    let stream = reader.unsplit(writer);
                    let acceptor = TlsAcceptor::from(cfg);
                    let tls = acceptor
                        .accept(stream)
                        .await
                        .map_err(|e| ChatmailError::protocol(format!("STARTTLS failed: {e}")))?;
                    self.seen_ehlo = false;
                    return self.handle_tls_connection(tls).await;
                }
                "AUTH" if line.to_ascii_uppercase().starts_with("AUTH PLAIN") => {
                    writer
                        .write_all(b"530 5.7.0 Must issue a STARTTLS command first\r\n")
                        .await?;
                }
                "QUIT" => {
                    writer.write_all(b"221 2.0.0 Bye\r\n").await?;
                    break;
                }
                "NOOP" => writer.write_all(b"250 2.0.0 OK\r\n").await?,
                _ => {
                    writer
                        .write_all(b"502 5.5.1 Command not implemented\r\n")
                        .await?;
                }
            }
        }
        Ok(())
    }

    fn format_ehlo(&self, tls_active: bool) -> String {
        let mut out = format!("250-{}\r\n", self.cfg.hostname);
        if !tls_active && self.cfg.starttls_config.is_some() {
            out.push_str("250-STARTTLS\r\n");
        }
        out.push_str(&format!(
            "250-SIZE {}\r\n",
            self.ctx.message_size.effective()
        ));
        if tls_active || self.cfg.starttls_config.is_none() {
            out.push_str("250-AUTH PLAIN\r\n");
        }
        out.push_str("250 OK\r\n");
        out
    }

    async fn serve<R, W>(&mut self, reader: R, mut writer: W, tls_active: bool) -> Result<()>
    where
        R: tokio::io::AsyncRead + Unpin,
        W: AsyncWriteExt + Unpin,
    {
        let mut lines = BufReader::new(reader).lines();

        // RFC 8314: banner on cleartext and on implicit TLS (:465); skip duplicate after STARTTLS upgrade.
        if !tls_active || self.cfg.starttls_config.is_none() {
            writer
                .write_all(format!("220 {} ESMTP madmail-v2\r\n", self.cfg.hostname).as_bytes())
                .await?;
        }

        while let Some(line) = lines.next_line().await? {
            let line = line.trim_end().to_string();
            if line.is_empty() {
                continue;
            }
            let cmd = line
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            match cmd.as_str() {
                "EHLO" | "HELO" => {
                    self.seen_ehlo = true;
                    writer
                        .write_all(self.format_ehlo(tls_active).as_bytes())
                        .await?;
                }
                "STARTTLS" => {
                    if tls_active {
                        writer
                            .write_all(b"503 5.5.1 STARTTLS already active\r\n")
                            .await?;
                    } else {
                        writer
                            .write_all(b"502 5.5.1 STARTTLS not available\r\n")
                            .await?;
                    }
                }
                "AUTH" if line.to_ascii_uppercase().starts_with("AUTH PLAIN") => {
                    if self.cfg.starttls_config.is_some() && !tls_active {
                        writer
                            .write_all(b"530 5.7.0 Must issue a STARTTLS command first\r\n")
                            .await?;
                        continue;
                    }
                    let user = match parse_auth_plain(&line) {
                        Ok(u) => u,
                        Err(e) => {
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "AUTH",
                                501,
                                "5.5.1",
                            );
                            return Err(e);
                        }
                    };
                    let auth = AuthContext {
                        pool: self.pool.clone(),
                        state: Arc::clone(&self.ctx),
                        primary_domain: self.cfg.primary_domain.clone(),
                        jit_domain: self.cfg.jit_domain.clone(),
                        credential_policy: self.cfg.credential_policy,
                    };
                    if let Err(e) = chatmail_auth::authenticate(&auth, &user.0, &user.1).await {
                        chatmail_metrics::record_smtp_failed_login(self.cfg.module);
                        writer
                            .write_all(b"535 5.7.8 Invalid credentials\r\n")
                            .await?;
                        tracing::debug!(error = %e, "SMTP AUTH failed");
                        continue;
                    }
                    self.authenticated_user = Some(normalize_username(&user.0)?);
                    writer
                        .write_all(b"235 2.7.0 Authentication successful\r\n")
                        .await?;
                }
                "MAIL" => {
                    if !self.seen_ehlo {
                        writer.write_all(b"503 5.5.1 EHLO first\r\n").await?;
                        chatmail_metrics::record_smtp_failed_command(
                            self.cfg.module,
                            "MAIL",
                            503,
                            "5.5.1",
                        );
                        continue;
                    }
                    if self.cfg.require_auth && self.authenticated_user.is_none() {
                        writer
                            .write_all(b"530 5.7.0 Authentication required\r\n")
                            .await?;
                        chatmail_metrics::record_smtp_failed_command(
                            self.cfg.module,
                            "MAIL",
                            530,
                            "5.7.0",
                        );
                        continue;
                    }
                    match parse_path_addr(&line, "FROM:") {
                        Ok(addr) => self.mail_from = addr,
                        Err(e) => {
                            writer.write_all(b"501 5.5.4 Bad address\r\n").await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "MAIL",
                                501,
                                "5.5.4",
                            );
                            return Err(e);
                        }
                    }
                    let declared = match parse_smtp_size_parameter(&line) {
                        Ok(s) => s,
                        Err(_) => {
                            writer.write_all(b"501 5.5.4 Bad SIZE\r\n").await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "MAIL",
                                501,
                                "5.5.4",
                            );
                            self.mail_from.clear();
                            continue;
                        }
                    };
                    if let Some(declared) = declared {
                        if declared > self.ctx.message_size.effective() {
                            writer
                                .write_all(
                                    format!("{}\r\n", chatmail_types::MESSAGE_FILE_TOO_BIG)
                                        .as_bytes(),
                                )
                                .await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "MAIL",
                                552,
                                "5.3.4",
                            );
                            self.mail_from.clear();
                            continue;
                        }
                    }
                    if !self.cfg.require_auth {
                        let policy_mode = self.ctx.federation_policy.global_mode();
                        if check_inbound_mail_from(
                            &self.mail_from,
                            &self.ctx.federation_policy,
                            &self.cfg.local_domains,
                            policy_mode,
                        )
                        .is_err()
                        {
                            writer.write_all(b"554 5.7.1 Policy Rejection\r\n").await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "MAIL",
                                554,
                                "5.7.1",
                            );
                            self.mail_from.clear();
                            continue;
                        }
                    }
                    chatmail_metrics::record_smtp_started(self.cfg.module);
                    writer.write_all(b"250 2.1.0 OK\r\n").await?;
                }
                "RCPT" => {
                    if self.mail_from.is_empty() {
                        writer.write_all(b"503 5.5.1 MAIL first\r\n").await?;
                        chatmail_metrics::record_smtp_failed_command(
                            self.cfg.module,
                            "RCPT",
                            503,
                            "5.5.1",
                        );
                        continue;
                    }
                    let rcpt = match parse_path_addr(&line, "TO:") {
                        Ok(a) => match normalize_username(&a) {
                            Ok(r) => r,
                            Err(e) => {
                                writer.write_all(b"501 5.5.4 Bad address\r\n").await?;
                                chatmail_metrics::record_smtp_failed_command(
                                    self.cfg.module,
                                    "RCPT",
                                    501,
                                    "5.5.4",
                                );
                                return Err(e);
                            }
                        },
                        Err(e) => {
                            writer.write_all(b"501 5.5.4 Bad address\r\n").await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "RCPT",
                                501,
                                "5.5.4",
                            );
                            return Err(e);
                        }
                    };
                    if let Err(ChatmailError::FederationRejected(_)) =
                        check_outbound_rcpt_federation(
                            &self.ctx.federation_policy,
                            &self.cfg.local_domains,
                            &rcpt,
                        )
                    {
                        writer.write_all(b"550 5.7.1 Policy Rejection\r\n").await?;
                        chatmail_metrics::record_smtp_failed_command(
                            self.cfg.module,
                            "RCPT",
                            550,
                            "5.7.1",
                        );
                        continue;
                    }
                    self.rcpt_to.push(rcpt);
                    writer.write_all(b"250 2.1.5 OK\r\n").await?;
                }
                "DATA" => {
                    if self.rcpt_to.is_empty() {
                        writer.write_all(b"503 5.5.1 RCPT first\r\n").await?;
                        chatmail_metrics::record_smtp_failed_command(
                            self.cfg.module,
                            "DATA",
                            503,
                            "5.5.1",
                        );
                        continue;
                    }
                    writer.write_all(b"354 Start mail input\r\n").await?;
                    let max_bytes = self.ctx.message_size.effective();
                    let data = match read_smtp_data_limited(&mut lines, max_bytes).await {
                        Ok(d) => d,
                        Err(ChatmailError::MessageTooLarge) => {
                            writer
                                .write_all(
                                    format!("{}\r\n", chatmail_types::MESSAGE_FILE_TOO_BIG)
                                        .as_bytes(),
                                )
                                .await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "DATA",
                                552,
                                "5.3.4",
                            );
                            self.mail_from.clear();
                            self.rcpt_to.clear();
                            continue;
                        }
                        Err(e) => return Err(e),
                    };
                    match self.ingest_data(&data).await {
                        Ok(()) => {
                            chatmail_metrics::record_smtp_completed(self.cfg.module);
                            writer.write_all(b"250 2.0.0 OK\r\n").await?;
                        }
                        Err(ChatmailError::EncryptionNeeded(_)) => {
                            writer.write_all(b"523 5.7.1 Encryption Needed\r\n").await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "DATA",
                                523,
                                "5.7.1",
                            );
                        }
                        Err(ChatmailError::MessageTooLarge) => {
                            writer
                                .write_all(
                                    format!("{}\r\n", chatmail_types::MESSAGE_FILE_TOO_BIG)
                                        .as_bytes(),
                                )
                                .await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "DATA",
                                552,
                                "5.3.4",
                            );
                        }
                        Err(ChatmailError::QuotaExceeded { .. }) => {
                            writer.write_all(b"552 5.2.2 Quota exceeded\r\n").await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "DATA",
                                552,
                                "5.2.2",
                            );
                        }
                        Err(ChatmailError::FederationRejected(_)) => {
                            writer.write_all(b"550 5.7.1 Policy Rejection\r\n").await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "DATA",
                                550,
                                "5.7.1",
                            );
                        }
                        Err(ChatmailError::Protocol(_)) => {
                            writer
                                .write_all(
                                    b"554 5.6.0 From header does not match envelope sender\r\n",
                                )
                                .await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "DATA",
                                554,
                                "5.6.0",
                            );
                        }
                        Err(_) => {
                            writer.write_all(b"451 4.0.0 Temporary failure\r\n").await?;
                            chatmail_metrics::record_smtp_failed_command(
                                self.cfg.module,
                                "DATA",
                                451,
                                "4.0.0",
                            );
                        }
                    }
                    self.mail_from.clear();
                    self.rcpt_to.clear();
                }
                "QUIT" => {
                    writer.write_all(b"221 2.0.0 Bye\r\n").await?;
                    break;
                }
                "RSET" => {
                    if !self.mail_from.is_empty() || !self.rcpt_to.is_empty() {
                        chatmail_metrics::record_smtp_aborted(self.cfg.module);
                    }
                    self.mail_from.clear();
                    self.rcpt_to.clear();
                    writer.write_all(b"250 2.0.0 OK\r\n").await?;
                }
                "NOOP" => writer.write_all(b"250 2.0.0 OK\r\n").await?,
                _ => {
                    writer
                        .write_all(b"502 5.5.1 Command not implemented\r\n")
                        .await?
                }
            }
        }
        Ok(())
    }

    async fn ingest_data(&self, data: &[u8]) -> Result<()> {
        self.ctx.check_message_size(data.len())?;
        if self.cfg.require_auth {
            validate_submission_headers(data, &self.mail_from)?;
        } else if chatmail_db::is_federation_sender_blocked(&self.mail_from) {
            tracing::debug!(from = %self.mail_from, "silently dropped inbound from blocked sender");
            return Ok(());
        }

        enforce_encryption(
            data,
            &EnforceOptions {
                mail_from: self.mail_from.clone(),
                recipients: self.rcpt_to.clone(),
            },
        )?;

        let delivery = DeliveryContext {
            pool: self.pool.clone(),
            state: Arc::clone(&self.ctx),
            primary_domain: self.cfg.primary_domain.clone(),
            local_domains: self.cfg.local_domains.clone(),
        };

        let ingest_start = std::time::Instant::now();
        let total_rcpts = self.rcpt_to.len();
        let mut local_deliveries: Vec<(String, String)> = Vec::new();

        for rcpt in &self.rcpt_to {
            let rcpt = normalize_username(rcpt)?;
            self.ctx.quota.check_quota(&rcpt, data.len() as u64)?;
            if delivery.is_local(&rcpt) {
                if !self.cfg.require_auth && !self.ctx.auth.local_recipient_allowed(&rcpt) {
                    tracing::debug!(rcpt = %rcpt, "silently dropped inbound local delivery");
                    continue;
                }
                local_deliveries.push((rcpt, uuid::Uuid::new_v4().to_string()));
            } else if self
                .ctx
                .federation_silent_dismiss
                .is_dismissed(&rcpt, &self.cfg.local_domains)
            {
                tracing::debug!(rcpt = %rcpt, "silently dismissed outbound federation message");
            } else {
                delivery
                    .enqueue_remote(self.mail_from.clone(), rcpt.clone(), data.to_vec())
                    .await?;
            }
        }

        let rcpt_phase = ingest_start.elapsed();
        if !local_deliveries.is_empty() {
            let local_n = local_deliveries.len();
            let deliver_start = std::time::Instant::now();
            let outcome =
                deliver_local_messages(&self.ctx.mailbox_store, &local_deliveries, data).await?;
            let deliver_ms = deliver_start.elapsed();
            let notify_start = std::time::Instant::now();
            // Notify (and charge quota) only for recipients whose body is durably on disk. Each
            // write/link above already fsync'd the maildir directory, so notification strictly
            // follows durability.
            for (rcpt, msg_id) in &outcome.delivered {
                self.ctx.quota.record_write(rcpt, data.len() as u64);
                self.ctx.events.notify_new_message(rcpt, msg_id);
                self.ctx
                    .notify_inbound_push(&self.pool, &self.mail_from, rcpt)
                    .await;
            }
            if !outcome.failed.is_empty() {
                for (rcpt, _msg_id, err) in &outcome.failed {
                    tracing::warn!(rcpt = %rcpt, error = %err, "local delivery failed for recipient");
                }
                tracing::warn!(
                    delivered = outcome.delivered.len(),
                    failed = outcome.failed.len(),
                    "partial local fan-out: some recipients did not receive the message"
                );
            }
            tracing::info!(
                total_rcpts,
                local_n,
                delivered = outcome.delivered.len(),
                failed = outcome.failed.len(),
                rcpt_phase_ms = rcpt_phase.as_millis(),
                deliver_ms = deliver_ms.as_millis(),
                notify_ms = notify_start.elapsed().as_millis(),
                ingest_ms = ingest_start.elapsed().as_millis(),
                "SMTP local fan-out timing"
            );
        }
        if !self.cfg.require_auth {
            chatmail_db::record_inbound_delivery();
            if let Some((_, sender_domain)) = self.mail_from.rsplit_once('@') {
                let sender_domain = sender_domain.to_ascii_lowercase();
                if !sender_domain.is_empty() {
                    self.ctx
                        .federation_tracker
                        .record_success(&sender_domain, 0, "");
                }
            }
        }
        chatmail_db::record_smtp_accepted(self.cfg.require_auth);
        Ok(())
    }
}

fn parse_path_addr(line: &str, prefix: &str) -> Result<String> {
    let upper = line.to_ascii_uppercase();
    let idx = upper
        .find(prefix)
        .ok_or_else(|| ChatmailError::protocol("bad address"))?;
    let rest = line[idx + prefix.len()..].trim();
    let addr = rest.trim_start_matches('<').trim_end_matches('>');
    Ok(addr.to_string())
}

fn parse_auth_plain(line: &str) -> Result<(String, String)> {
    let b64 = line
        .split_whitespace()
        .nth(2)
        .ok_or_else(|| ChatmailError::protocol("AUTH PLAIN missing payload"))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| ChatmailError::protocol(e.to_string()))?;
    let s = String::from_utf8_lossy(&decoded);
    let mut parts = s.split('\0');
    let _authz = parts.next();
    let user = parts
        .next()
        .ok_or_else(|| ChatmailError::protocol("no user"))?;
    let pass = parts
        .next()
        .ok_or_else(|| ChatmailError::protocol("no pass"))?;
    Ok((user.to_string(), pass.to_string()))
}

pub const PGP_MIME_BODY: &[u8] = b"From: sender@test\r\nTo: rcpt@test\r\nSubject: e\r\nContent-Type: multipart/encrypted; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use chatmail_auth::hash_password;
    use std::net::TcpListener as StdListener;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    #[tokio::test]
    async fn p4_ut03_test_smtp_state_machine_order() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let mut s = SmtpSession {
            ctx: Arc::new(chatmail_state::AppState::new("/tmp", pool.clone())),
            pool,
            cfg: SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: true,
                module: "submission",
                starttls_config: None,
            },
            authenticated_user: None,
            mail_from: String::new(),
            rcpt_to: Vec::new(),
            seen_ehlo: false,
        };
        assert!(!s.seen_ehlo);
        s.seen_ehlo = true;
        s.mail_from = "a@test".into();
        s.rcpt_to.push("b@test".into());
        assert!(!s.mail_from.is_empty() && !s.rcpt_to.is_empty());
    }

    async fn smtp_dialog(
        cfg: SmtpSessionConfig,
        pool: DbPool,
        ctx: Arc<AppState>,
        script: &[&str],
    ) -> String {
        ctx.auth.hydrate(&pool).await.unwrap();

        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();

        let pool_bg = pool.clone();
        let ctx_bg = Arc::clone(&ctx);
        let cfg_bg = cfg.clone();
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = SmtpSession::new(ctx_bg, pool_bg, cfg_bg);
            let _ = session.handle_connection(stream).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut transcript = String::new();
        let mut buf = [0u8; 4096];

        for line in script {
            if *line == ".DATA_END" {
                stream.write_all(b".\r\n").await.unwrap();
                tokio::time::sleep(Duration::from_millis(40)).await;
                transcript.push_str(&read_smtp_chunk(&mut stream, &mut buf).await);
            } else if let Some(body) = line.strip_prefix("DATA:") {
                for part in body.split("\r\n") {
                    if part.is_empty() {
                        continue;
                    }
                    stream.write_all(part.as_bytes()).await.unwrap();
                    stream.write_all(b"\r\n").await.unwrap();
                }
            } else {
                stream.write_all(line.as_bytes()).await.unwrap();
                stream.write_all(b"\r\n").await.unwrap();
                tokio::time::sleep(Duration::from_millis(40)).await;
                transcript.push_str(&read_smtp_chunk(&mut stream, &mut buf).await);
            }
        }
        transcript
    }

    async fn read_smtp_chunk(stream: &mut TcpStream, buf: &mut [u8; 4096]) -> String {
        tokio::time::timeout(Duration::from_secs(3), async {
            let mut acc = String::new();
            loop {
                let n = stream.read(buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                if acc.contains("250 ")
                    || acc.contains("235 ")
                    || acc.contains("354 ")
                    || acc.contains("523 ")
                    || acc.contains("552 ")
                    || acc.contains("554 ")
                    || acc.contains("530 ")
                    || acc.contains("503 ")
                    || acc.contains("221 ")
                {
                    break;
                }
            }
            acc
        })
        .await
        .unwrap_or_default()
    }

    #[tokio::test]
    async fn p4_ut01_smtp_rejects_plaintext_with_523() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(std::env::temp_dir(), pool.clone()));
        let t = smtp_dialog(
            SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: false,
                module: "smtp",
                starttls_config: None,
            },
            pool,
            ctx,
            &[
                "EHLO client.test",
                "MAIL FROM:<sender@test>",
                "RCPT TO:<rcpt@test>",
                "DATA",
                "DATA:From: sender@test\r\nTo: rcpt@test\r\nSubject: x\r\nContent-Type: text/plain\r\n\r\nhi",
                ".DATA_END",
            ],
        )
        .await;
        assert!(t.contains("523"), "got: {t}");
    }

    #[tokio::test]
    async fn p4_submission_from_envelope_mismatch_554() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("secret").unwrap();
        chatmail_db::passwords::create_user(&pool, "sender@test", &hash)
            .await
            .unwrap();
        let ctx = Arc::new(AppState::new(std::env::temp_dir(), pool.clone()));
        let b64 = base64::engine::general_purpose::STANDARD.encode("\0sender@test\0secret");
        let auth = format!("AUTH PLAIN {b64}");
        let t = smtp_dialog(
            SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: true,
                module: "submission",
                starttls_config: None,
            },
            pool,
            ctx,
            &[
                "EHLO client.test",
                &auth,
                "MAIL FROM:<sender@test>",
                "RCPT TO:<sender@test>",
                "DATA",
                "DATA:From: other@test\r\nTo: sender@test\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n",
                ".DATA_END",
            ],
        )
        .await;
        assert!(t.contains("554 5.6.0"), "got: {t}");
    }

    #[tokio::test]
    async fn p4_inbound_federation_rejects_mail_from() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(std::env::temp_dir(), pool.clone()));
        ctx.federation_policy.add_exception("evil.test");
        let t = smtp_dialog(
            SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: false,
                module: "smtp",
                starttls_config: None,
            },
            pool,
            ctx,
            &["EHLO x", "MAIL FROM:<a@evil.test>"],
        )
        .await;
        assert!(t.contains("554 5.7.1"), "got: {t}");
    }

    #[tokio::test]
    async fn submission_silent_dismiss_accepts_without_local_delivery() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("secret").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();

        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        ctx.federation_silent_dismiss
            .add(&pool, "1.1.1.1")
            .await
            .unwrap();

        let b64 = base64::engine::general_purpose::STANDARD.encode("\0u@test\0secret");
        let auth = format!("AUTH PLAIN {b64}");
        let body = std::str::from_utf8(PGP_MIME_BODY)
            .unwrap()
            .replace("sender@test", "u@test")
            .replace("rcpt@test", "peer@[1.1.1.1]");
        let t = smtp_dialog(
            SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: true,
                module: "submission",
                starttls_config: None,
            },
            pool,
            ctx.clone(),
            &[
                "EHLO client.test",
                &auth,
                "MAIL FROM:<u@test>",
                "RCPT TO:<peer@[1.1.1.1]>",
                "DATA",
                &format!("DATA:{body}"),
                ".DATA_END",
                "QUIT",
            ],
        )
        .await;
        assert!(t.contains("250 2.1.5 OK"), "RCPT should succeed: {t}");
        assert!(t.contains("250 2.0.0 OK"), "DATA should succeed: {t}");
        let paths = ctx.mailbox_store.maildir_for_user("u@test");
        let n_new = std::fs::read_dir(&paths.new)
            .map(|d| d.count())
            .unwrap_or(0);
        let n_cur = std::fs::read_dir(&paths.cur)
            .map(|d| d.count())
            .unwrap_or(0);
        assert_eq!(n_new + n_cur, 0, "remote dismiss must not store locally");
    }

    #[tokio::test]
    async fn p4_submission_delivers_encrypted_message() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("secret").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();

        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        let b64 = base64::engine::general_purpose::STANDARD.encode("\0u@test\0secret");
        let auth = format!("AUTH PLAIN {b64}");
        let body = std::str::from_utf8(PGP_MIME_BODY)
            .unwrap()
            .replace("sender@test", "u@test");
        let t = smtp_dialog(
            SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: true,
                module: "submission",
                starttls_config: None,
            },
            pool,
            ctx.clone(),
            &[
                "EHLO client.test",
                &auth,
                "MAIL FROM:<u@test>",
                "RCPT TO:<u@test>",
                "DATA",
                &format!("DATA:{body}"),
                ".DATA_END",
                "QUIT",
            ],
        )
        .await;
        assert!(t.contains("250 2.0.0 OK"), "got: {t}");
        let paths = ctx.mailbox_store.maildir_for_user("u@test");
        let n_new = std::fs::read_dir(&paths.new)
            .map(|d| d.count())
            .unwrap_or(0);
        let n_cur = std::fs::read_dir(&paths.cur)
            .map(|d| d.count())
            .unwrap_or(0);
        assert!(
            n_new + n_cur >= 1,
            "expected maildir message in new/ or cur/"
        );
    }

    #[tokio::test]
    async fn p4_smtp_data_rejects_message_file_too_big() {
        use chatmail_config::AppConfig;
        use chatmail_types::MESSAGE_FILE_TOO_BIG;

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let mut cfg = AppConfig::default();
        cfg.appendlimit = Some("2K".into());
        cfg.max_message_size = Some("2K".into());
        let ctx = Arc::new(AppState::with_quota_and_message_limit(
            dir.path(),
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &cfg,
            pool.clone(),
        ));
        ctx.hydrate(&pool, &cfg).await.unwrap();

        let payload = "x".repeat(3000);
        let t = smtp_dialog(
            SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: false,
                module: "smtp",
                starttls_config: None,
            },
            pool,
            ctx,
            &[
                "EHLO client.test",
                "MAIL FROM:<a@test>",
                "RCPT TO:<b@test>",
                "DATA",
                &format!("DATA:{payload}"),
                ".DATA_END",
            ],
        )
        .await;
        assert!(
            t.contains(MESSAGE_FILE_TOO_BIG),
            "expected size rejection, got: {t}"
        );
    }

    #[tokio::test]
    async fn p4_smtp_ehlo_advertises_configured_size() {
        use chatmail_config::AppConfig;

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let mut cfg = AppConfig::default();
        cfg.max_message_size = Some("100M".into());
        let ctx = Arc::new(AppState::with_quota_and_message_limit(
            dir.path(),
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &cfg,
            pool.clone(),
        ));
        ctx.hydrate(&pool, &cfg).await.unwrap();

        let t = smtp_dialog(
            SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: false,
                module: "smtp",
                starttls_config: None,
            },
            pool,
            ctx,
            &["EHLO client.test"],
        )
        .await;
        assert!(t.contains("SIZE 104857600"), "got: {t}");
    }

    #[tokio::test]
    async fn p4_smtp_mail_from_size_rejects_before_data() {
        use chatmail_config::AppConfig;
        use chatmail_types::MESSAGE_FILE_TOO_BIG;

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let mut cfg = AppConfig::default();
        cfg.appendlimit = Some("2K".into());
        cfg.max_message_size = Some("2K".into());
        let ctx = Arc::new(AppState::with_quota_and_message_limit(
            dir.path(),
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &cfg,
            pool.clone(),
        ));
        ctx.hydrate(&pool, &cfg).await.unwrap();

        let t = smtp_dialog(
            SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: false,
                module: "smtp",
                starttls_config: None,
            },
            pool,
            ctx,
            &[
                "EHLO client.test",
                "MAIL FROM:<a@test> SIZE=999999",
                "RCPT TO:<b@test>",
            ],
        )
        .await;
        assert!(
            t.contains(MESSAGE_FILE_TOO_BIG),
            "expected MAIL SIZE rejection, got: {t}"
        );
        assert!(!t.contains("354"), "should not reach DATA, got: {t}");
    }

    #[tokio::test]
    async fn inbound_silently_drops_unknown_local_user() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        let body = std::str::from_utf8(PGP_MIME_BODY).unwrap();
        let t = smtp_dialog(
            SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: false,
                module: "smtp",
                starttls_config: None,
            },
            pool,
            ctx.clone(),
            &[
                "EHLO client.test",
                "MAIL FROM:<sender@peer.test>",
                "RCPT TO:<ghost@test>",
                "DATA",
                &format!("DATA:{body}"),
                ".DATA_END",
            ],
        )
        .await;
        assert!(t.contains("250 2.0.0 OK"), "got: {t}");
        let paths = ctx.mailbox_store.maildir_for_user("ghost@test");
        let n = std::fs::read_dir(&paths.new)
            .map(|d| d.count())
            .unwrap_or(0)
            + std::fs::read_dir(&paths.cur)
                .map(|d| d.count())
                .unwrap_or(0);
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn inbound_silently_drops_admin_sender() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("x").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        let body = std::str::from_utf8(PGP_MIME_BODY)
            .unwrap()
            .replace("sender@test", "admin@peer.test")
            .replace("rcpt@test", "u@test");
        let t = smtp_dialog(
            SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: false,
                module: "smtp",
                starttls_config: None,
            },
            pool,
            ctx.clone(),
            &[
                "EHLO client.test",
                "MAIL FROM:<admin@peer.test>",
                "RCPT TO:<u@test>",
                "DATA",
                &format!("DATA:{body}"),
                ".DATA_END",
            ],
        )
        .await;
        assert!(t.contains("250 2.0.0 OK"), "got: {t}");
        let paths = ctx.mailbox_store.maildir_for_user("u@test");
        let n = std::fs::read_dir(&paths.new)
            .map(|d| d.count())
            .unwrap_or(0)
            + std::fs::read_dir(&paths.cur)
                .map(|d| d.count())
                .unwrap_or(0);
        assert_eq!(n, 0);
    }

    fn loopback_tls_configs() -> (Arc<ServerConfig>, Arc<rustls::ClientConfig>) {
        use rcgen::generate_simple_self_signed;
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};
        use rustls::{ClientConfig, RootCertStore};

        let rc = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert = CertificateDer::from(rc.cert.der().to_vec());
        let key = PrivateKeyDer::Pkcs8(rc.key_pair.serialize_der().into());
        let server = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert.clone()], key)
                .unwrap(),
        );
        let mut roots = RootCertStore::empty();
        roots.add(cert).unwrap();
        let client = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        (server, client)
    }

    async fn read_smtp_until<S>(stream: &mut S, needle: &str) -> String
    where
        S: AsyncReadExt + Unpin,
    {
        let mut buf = [0u8; 4096];
        tokio::time::timeout(Duration::from_secs(3), async {
            let mut acc = String::new();
            loop {
                let n = stream.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                if acc.contains(needle) {
                    break;
                }
            }
            acc
        })
        .await
        .unwrap_or_default()
    }

    async fn smtp_write(stream: &mut (impl AsyncWriteExt + Unpin), line: &str) {
        stream.write_all(line.as_bytes()).await.unwrap();
        stream.write_all(b"\r\n").await.unwrap();
    }

    #[tokio::test]
    async fn starttls_ehlo_advertises_starttls_before_tls() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let s = SmtpSession::new(
            Arc::new(chatmail_state::AppState::new(
                std::env::temp_dir(),
                pool.clone(),
            )),
            pool,
            SmtpSessionConfig {
                hostname: "mx.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: true,
                module: "submission",
                starttls_config: Some(loopback_tls_configs().0),
            },
        );
        let plain = s.format_ehlo(false);
        assert!(plain.contains("250-STARTTLS\r\n"));
        assert!(!plain.contains("AUTH PLAIN"));
        let tls = s.format_ehlo(true);
        assert!(!tls.contains("STARTTLS"));
        assert!(tls.contains("250-AUTH PLAIN\r\n"));
    }

    /// RFC 3207 / 8314: cleartext EHLO offers STARTTLS; AUTH blocked until upgrade.
    #[tokio::test]
    async fn submission_starttls_upgrade_then_auth_allowed() {
        use rustls::pki_types::ServerName;
        use tokio_rustls::TlsConnector;

        let (tls_server, tls_client) = loopback_tls_configs();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(std::env::temp_dir(), pool.clone()));
        let cfg = SmtpSessionConfig {
            hostname: "mx.test".into(),
            primary_domain: "test".into(),
            local_domains: vec!["test".into()],
            jit_domain: None,
            credential_policy: CredentialPolicy::default(),
            require_auth: true,
            module: "submission",
            starttls_config: Some(tls_server),
        };

        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();
        let pool_bg = pool.clone();
        let ctx_bg = Arc::clone(&ctx);
        let cfg_bg = cfg.clone();
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = SmtpSession::new(ctx_bg, pool_bg, cfg_bg);
            let _ = session.handle_connection(stream).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        let banner = read_smtp_until(&mut stream, "220 ").await;
        assert!(banner.contains("220 mx.test"), "banner: {banner}");

        smtp_write(&mut stream, "EHLO client.test").await;
        let ehlo = read_smtp_until(&mut stream, "250 OK").await;
        assert!(ehlo.contains("STARTTLS"), "pre-TLS EHLO: {ehlo}");
        assert!(!ehlo.contains("AUTH PLAIN"), "pre-TLS EHLO: {ehlo}");

        smtp_write(&mut stream, "AUTH PLAIN").await;
        let auth_reject = read_smtp_until(&mut stream, "530 ").await;
        assert!(
            auth_reject.contains("Must issue a STARTTLS"),
            "auth reject: {auth_reject}"
        );

        smtp_write(&mut stream, "STARTTLS").await;
        let ready = read_smtp_until(&mut stream, "Ready to start TLS").await;
        assert!(ready.contains("220 2.0.0"), "STARTTLS ready: {ready}");

        let connector = TlsConnector::from(tls_client);
        let server_name = ServerName::try_from("localhost").unwrap();
        let mut tls = connector.connect(server_name, stream).await.unwrap();

        smtp_write(&mut tls, "EHLO client.test").await;
        let ehlo_tls = read_smtp_until(&mut tls, "250 OK").await;
        assert!(ehlo_tls.contains("AUTH PLAIN"), "post-TLS EHLO: {ehlo_tls}");
        assert!(!ehlo_tls.contains("STARTTLS"), "post-TLS EHLO: {ehlo_tls}");
    }

    /// RFC 8314: implicit TLS on :465 must emit 220 banner before client EHLO.
    #[tokio::test]
    async fn submission_implicit_tls_sends_banner_before_ehlo() {
        use rustls::pki_types::ServerName;
        use tokio_rustls::TlsAcceptor;
        use tokio_rustls::TlsConnector;

        let (tls_server, tls_client) = loopback_tls_configs();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(std::env::temp_dir(), pool.clone()));
        let cfg = SmtpSessionConfig {
            hostname: "mx.test".into(),
            primary_domain: "test".into(),
            local_domains: vec!["test".into()],
            jit_domain: None,
            credential_policy: CredentialPolicy::default(),
            require_auth: true,
            module: "submission",
            starttls_config: None,
        };

        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();
        let acceptor = TlsAcceptor::from(Arc::clone(&tls_server));
        let pool_bg = pool.clone();
        let ctx_bg = Arc::clone(&ctx);
        let cfg_bg = cfg.clone();
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let tls = acceptor.accept(stream).await.unwrap();
            let mut session = SmtpSession::new(ctx_bg, pool_bg, cfg_bg);
            let _ = session.handle_tls_connection(tls).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let stream = TcpStream::connect(addr).await.unwrap();
        let connector = TlsConnector::from(tls_client);
        let server_name = ServerName::try_from("localhost").unwrap();
        let mut tls = connector.connect(server_name, stream).await.unwrap();

        let banner = read_smtp_until(&mut tls, "220 ").await;
        assert!(banner.contains("220 mx.test"), "banner: {banner}");

        smtp_write(&mut tls, "EHLO client.test").await;
        let ehlo = read_smtp_until(&mut tls, "250 OK").await;
        assert!(ehlo.contains("AUTH PLAIN"), "EHLO: {ehlo}");
    }
}
