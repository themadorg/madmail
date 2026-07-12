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

//! Outbound SMTP federation client (RFC 3207 STARTTLS when advertised).

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use chatmail_types::is_ipv4_literal;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use tracing::{debug, info, warn};

use crate::router::OutboundJob;

static FEDERATION_SMTP_TLS: OnceLock<TlsConnector> = OnceLock::new();

/// Process-wide SMTP TLS client (accepts self-signed / expired peer certs, like HTTP `/mxdeliv`).
fn federation_smtp_tls_connector() -> &'static TlsConnector {
    FEDERATION_SMTP_TLS.get_or_init(|| {
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();
        TlsConnector::from(Arc::new(config))
    })
}

#[derive(Debug)]
struct SkipServerVerification;

impl ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ED25519,
        ]
    }
}

enum SmtpTransport {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl SmtpTransport {
    async fn write_all(&mut self, data: impl AsRef<[u8]>) -> Result<(), String> {
        match self {
            Self::Plain(s) => s.write_all(data.as_ref()).await,
            Self::Tls(s) => s.write_all(data.as_ref()).await,
        }
        .map_err(|e| e.to_string())
    }

    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        match self {
            Self::Plain(s) => s.read(buf).await,
            Self::Tls(s) => s.read(buf).await,
        }
        .map_err(|e| e.to_string())
    }
}

/// Deliver one message over SMTP.
///
/// Tries port 25 (optional STARTTLS) first, then port 443 implicit TLS — required for
/// classic Chatmail relays (`nine.testrun.org`, etc.) that expose SMTP only on :443.
///
/// `helo_name` is this server's identity for EHLO (primary_domain / public IP), not the remote host.
pub async fn deliver(host: &str, job: &OutboundJob, helo_name: &str) -> Result<(), String> {
    let connect_host = host.trim_matches(|c| c == '[' || c == ']');
    let helo = if helo_name.trim().is_empty() {
        connect_host
    } else {
        helo_name.trim().trim_matches(|c| c == '[' || c == ']')
    };
    let rcpt_domain = job
        .rcpt_to
        .rsplit_once('@')
        .map(|(_, d)| d)
        .unwrap_or(connect_host);

    let endpoint25 = format!("{connect_host}:25");
    match deliver_plain_starttls(&endpoint25, connect_host, rcpt_domain, helo, job).await {
        Ok(()) => {
            info!(endpoint = %endpoint25, rcpt = %job.rcpt_to, "federation: SMTP delivery ok (port 25)");
            return Ok(());
        }
        Err(e25) => {
            warn!(
                endpoint = %endpoint25,
                rcpt = %job.rcpt_to,
                error = %e25,
                "federation: SMTP :25 failed, trying implicit TLS :443"
            );
        }
    }

    let endpoint443 = format!("{connect_host}:443");
    deliver_implicit_tls(&endpoint443, connect_host, rcpt_domain, helo, job)
        .await
        .map_err(|e443| format!("smtp :25 failed; smtp :443 tls: {e443}"))
}

/// Plain SMTP on :25 with optional STARTTLS (RFC 3207).
async fn deliver_plain_starttls(
    endpoint: &str,
    connect_host: &str,
    rcpt_domain: &str,
    helo_name: &str,
    job: &OutboundJob,
) -> Result<(), String> {
    debug!(endpoint, "federation: SMTP plain connect");

    let stream = tokio::time::timeout(Duration::from_secs(30), TcpStream::connect(endpoint))
        .await
        .map_err(|_| "smtp connect timeout".to_string())?
        .map_err(|e| format!("smtp connect: {e}"))?;

    let mut transport = SmtpTransport::Plain(stream);
    read_smtp_reply(&mut transport, 220).await?;

    transport
        .write_all(format!("EHLO {helo_name}\r\n"))
        .await?;
    let ehlo_plain = read_smtp_reply(&mut transport, 250).await?;

    if ehlo_advertises_starttls(&ehlo_plain) {
        debug!(endpoint, "federation: SMTP STARTTLS upgrade");
        transport.write_all(b"STARTTLS\r\n").await?;
        read_smtp_reply(&mut transport, 220).await?;

        let SmtpTransport::Plain(stream) = transport else {
            return Err("smtp starttls: expected plain transport".into());
        };
        let server_name = smtp_tls_server_name(connect_host, rcpt_domain)?;
        let tls_stream = tokio::time::timeout(
            Duration::from_secs(30),
            federation_smtp_tls_connector().connect(server_name, stream),
        )
        .await
        .map_err(|_| "smtp starttls timeout".to_string())?
        .map_err(|e| format!("smtp starttls: {e}"))?;

        transport = SmtpTransport::Tls(Box::new(tls_stream));
        transport
            .write_all(format!("EHLO {helo_name}\r\n"))
            .await?;
        read_smtp_reply(&mut transport, 250).await?;
    }

    run_smtp_transaction(&mut transport, job).await
}

/// Implicit TLS on :443 (Chatmail / Delta Chat relay SMTP submission style).
async fn deliver_implicit_tls(
    endpoint: &str,
    connect_host: &str,
    rcpt_domain: &str,
    helo_name: &str,
    job: &OutboundJob,
) -> Result<(), String> {
    info!(endpoint, rcpt = %job.rcpt_to, "federation: SMTP implicit TLS connect");

    let stream = tokio::time::timeout(Duration::from_secs(30), TcpStream::connect(endpoint))
        .await
        .map_err(|_| "smtp tls connect timeout".to_string())?
        .map_err(|e| format!("smtp tls connect: {e}"))?;

    let server_name = smtp_tls_server_name(connect_host, rcpt_domain)?;
    let tls_stream = tokio::time::timeout(
        Duration::from_secs(30),
        federation_smtp_tls_connector().connect(server_name, stream),
    )
    .await
    .map_err(|_| "smtp tls handshake timeout".to_string())?
    .map_err(|e| format!("smtp tls handshake: {e}"))?;

    let mut transport = SmtpTransport::Tls(Box::new(tls_stream));
    read_smtp_reply(&mut transport, 220).await?;
    transport
        .write_all(format!("EHLO {helo_name}\r\n"))
        .await?;
    read_smtp_reply(&mut transport, 250).await?;

    run_smtp_transaction(&mut transport, job).await?;
    info!(endpoint, rcpt = %job.rcpt_to, "federation: SMTP delivery ok (port 443 TLS)");
    Ok(())
}

async fn run_smtp_transaction(
    transport: &mut SmtpTransport,
    job: &OutboundJob,
) -> Result<(), String> {
    transport
        .write_all(format!("MAIL FROM:<{}>\r\n", job.mail_from))
        .await?;
    read_smtp_reply(transport, 250).await?;

    transport
        .write_all(format!("RCPT TO:<{}>\r\n", job.rcpt_to))
        .await?;
    read_smtp_reply(transport, 250).await?;

    transport.write_all(b"DATA\r\n").await?;
    read_smtp_reply(transport, 354).await?;

    transport.write_all(&job.data).await?;
    if !job.data.ends_with(b"\r\n") {
        transport.write_all(b"\r\n").await?;
    }
    transport.write_all(b".\r\n").await?;
    read_smtp_reply(transport, 250).await?;

    transport.write_all(b"QUIT\r\n").await?;
    let _ = read_smtp_reply(transport, 221).await;
    Ok(())
}

/// Legacy name used by unit tests.
#[cfg(test)]
async fn deliver_to_endpoint(
    endpoint: &str,
    connect_host: &str,
    rcpt_domain: &str,
    job: &OutboundJob,
) -> Result<(), String> {
    deliver_plain_starttls(endpoint, connect_host, rcpt_domain, connect_host, job).await
}

fn smtp_tls_server_name(
    connect_host: &str,
    rcpt_domain: &str,
) -> Result<ServerName<'static>, String> {
    let bare = connect_host.trim().trim_matches(|c| c == '[' || c == ']');
    let name = if is_ipv4_literal(bare) {
        rcpt_domain.trim().trim_matches(|c| c == '[' || c == ']')
    } else {
        bare
    };
    ServerName::try_from(name.to_string()).map_err(|e| format!("smtp tls server name: {e}"))
}

fn ehlo_advertises_starttls(ehlo_response: &str) -> bool {
    ehlo_response.lines().any(|line| {
        let upper = line.to_ascii_uppercase();
        upper.starts_with("250") && upper.contains("STARTTLS")
    })
}

async fn read_smtp_reply(
    transport: &mut SmtpTransport,
    expect_code: u16,
) -> Result<String, String> {
    let mut acc = String::new();
    let mut buf = [0u8; 4096];
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let n = transport.read(&mut buf).await?;
            if n == 0 {
                return Err("smtp: connection closed".into());
            }
            acc.push_str(&String::from_utf8_lossy(&buf[..n]));
            if let Some(err) = smtp_final_line_error(&acc, expect_code) {
                return Err(err);
            }
            if smtp_has_final_line(&acc, expect_code) {
                return Ok(acc);
            }
        }
    })
    .await
    .map_err(|_| "smtp read timeout".to_string())?
}

fn smtp_has_final_line(acc: &str, expect_code: u16) -> bool {
    acc.lines()
        .any(|line| smtp_line_code(line) == Some((expect_code, false)))
}

fn smtp_final_line_error(acc: &str, expect_code: u16) -> Option<String> {
    for line in acc.lines() {
        let Some((code, continued)) = smtp_line_code(line) else {
            continue;
        };
        if continued {
            continue;
        }
        if code == expect_code {
            return None;
        }
        if code >= 400 {
            return Some(format!("smtp expected {expect_code}, got: {line}"));
        }
    }
    None
}

fn smtp_line_code(line: &str) -> Option<(u16, bool)> {
    let trimmed = line.trim_end();
    if trimmed.len() < 3 {
        return None;
    }
    let code: u16 = trimmed[..3].parse().ok()?;
    let continued = trimmed.len() > 3 && trimmed.as_bytes()[3] == b'-';
    Some((code, continued))
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener as StdListener;
    use std::sync::Arc;
    use std::time::Duration;

    use chatmail_config::CredentialPolicy;
    use chatmail_smtp::session::PGP_MIME_BODY;
    use chatmail_smtp::{SmtpSession, SmtpSessionConfig};
    use chatmail_state::AppState;
    use rcgen::generate_simple_self_signed;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use rustls::{ClientConfig, RootCertStore, ServerConfig};
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn ehlo_detects_starttls_extension() {
        let ehlo = "250-mx.test\r\n250-STARTTLS\r\n250 OK\r\n";
        assert!(ehlo_advertises_starttls(ehlo));
        assert!(!ehlo_advertises_starttls("250-mx.test\r\n250 OK\r\n"));
    }

    fn loopback_tls_configs() -> (Arc<ServerConfig>, Arc<ClientConfig>) {
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

    /// Peers that require STARTTLS before MAIL (e.g. Postfix `smtpd_tls_auth_only`) must work.
    #[tokio::test]
    async fn outbound_starttls_delivery_when_peer_requires_tls() {
        let (tls_server, _tls_client) = loopback_tls_configs();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(std::env::temp_dir(), pool.clone()));
        let cfg = SmtpSessionConfig {
            hostname: "mx.test".into(),
            primary_domain: "test".into(),
            local_domains: vec!["test".into()],
            jit_domain: None,
            credential_policy: CredentialPolicy::default(),
            require_auth: false,
            module: "smtp",
            starttls_config: Some(tls_server),
        };

        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();
        let pool_bg = pool.clone();
        let ctx_bg = Arc::clone(&ctx);
        let cfg_bg = cfg.clone();
        tokio::spawn(async move {
            let listener = TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = SmtpSession::new(ctx_bg, pool_bg, cfg_bg);
            let _ = session.handle_connection(stream).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;

        let data = std::str::from_utf8(PGP_MIME_BODY)
            .unwrap()
            .replace("sender@test", "sender@other.test")
            .into_bytes();
        let job = OutboundJob {
            mail_from: "sender@other.test".into(),
            rcpt_to: "rcpt@test".into(),
            data,
        };

        deliver_to_endpoint(&addr.to_string(), "mx.test", "test", &job)
            .await
            .expect("STARTTLS SMTP federation delivery");
    }
}
