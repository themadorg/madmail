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

//! One TLS port serving HTTPS, IMAP and submission.
//!
//! This module also owns the ALPN token policy for the *dedicated* mail ports —
//! see [`load_mail_tls_configs`] — so every listener presenting the shared
//! certificate agrees on which protocols it will speak.
//!
//! Upstream chatmail does this in nginx's `stream` module with `ssl_preread`,
//! passing raw TCP through to Dovecot/Postfix. Madmail is a single binary with no
//! proxy in front, so it terminates TLS once and dispatches in process. Doing so
//! also lets rustls enforce ALPN strictly — a client whose offers do not intersect
//! ours gets a fatal `no_application_protocol`, per RFC 9325 §3.8 — which the
//! passthrough design cannot do.
//!
//! # Identifying a connection
//!
//! Three signals, in this order. The order is the security property, not a
//! preference.
//!
//! 1. **ALPN**, from the ClientHello. Decisive whenever present.
//! 2. **SNI**, from the same ClientHello, consulted *only* when the client offered
//!    no ALPN. `imap.` / `smtp.` by default, or the hostnames an operator
//!    configured. This is what lets a stock mail client reach the port at all:
//!    Thunderbird, Apple Mail and K-9 send no ALPN, and Delta Chat sends one only
//!    on non-standard ports.
//! 3. **First bytes**, after the handshake, for a client that offered neither. Only
//!    a client that speaks *first* can be identified here — IMAP and SMTP servers
//!    must greet before the client may send a command — so this catches an early
//!    `EHLO`, a pipelined IMAP command, or an HTTP request line. Anything else,
//!    silence included, is served as HTTPS rather than guessed into a mail parser.
//!
//! Checking ALPN before SNI is what makes SNI routing safe here. SNI names a host,
//! not a protocol, and a browser sets it from the URL bar — so routing on SNI first
//! would drop a visitor to `https://imap.example.org/` into the IMAP parser with no
//! attacker involved. Every browser sends ALPN in order to negotiate HTTP/2, so
//! under this ordering a browser is always decided at step 1 and can never reach
//! step 2 or 3.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use chatmail_db::DbPool;
use chatmail_imap::{connection_stats, ImapSession, ImapSessionConfig};
use chatmail_smtp::{SmtpSession, SmtpSessionConfig};
use chatmail_state::AppState;
use chatmail_tls::{load_server_config, load_server_config_with_alpn};
use chatmail_types::Result;
use rustls::ServerConfig;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::LazyConfigAcceptor;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// ALPN token Delta Chat offers for IMAP on any port other than 993.
///
/// Fixed by the client, not by us — `chatmail/core` `src/imap/client.rs` returns
/// `"imap"`. IANA registers this identifier for IMAP (RFC 2595 reference).
pub const ALPN_IMAP: &str = "imap";

/// ALPN token Delta Chat offers for submission on any port other than 465.
///
/// `chatmail/core` `src/smtp/connect.rs` returns the bare token `"smtp"` — not
/// `submission`, and not `smtps`. There is no IANA-registered ALPN identifier for
/// SMTP or submission; this is an unregistered token shared with upstream chatmail.
pub const ALPN_SMTP: &str = "smtp";

/// ALPN token for HTTP, always advertised alongside the mail tokens.
///
/// `h2` is deliberately absent. Nothing negotiates ALPN on this port today, so
/// browsers already settle on HTTP/1.1; adding `h2` would switch the entire HTTPS
/// surface to HTTP/2 as a side effect of enabling mail.
const ALPN_HTTP: &str = "http/1.1";

/// Protocol a shared-port connection resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedProto {
    Imap,
    Submission,
    Http,
}

/// ALPN tokens to advertise, in server-preference order.
///
/// rustls selects the first of these the client also offers, so mail tokens come
/// before HTTP, and `smtp` before `imap` — the precedence upstream's nginx `map`
/// uses. Empty when neither mail protocol is enabled, which leaves the port
/// advertising no ALPN extension at all, exactly as before.
///
/// The tokens are constants rather than configuration: they are chosen by the
/// client, so an operator cannot usefully change them.
pub fn alpn_tokens(imap: bool, smtp: bool) -> Vec<Vec<u8>> {
    if !imap && !smtp {
        return Vec::new();
    }
    smtp.then_some(ALPN_SMTP)
        .into_iter()
        .chain(imap.then_some(ALPN_IMAP))
        .chain(std::iter::once(ALPN_HTTP))
        .map(|t| t.as_bytes().to_vec())
        .collect()
}

/// Map the negotiated ALPN token to a protocol. Anything unrecognised is HTTPS.
pub fn route(negotiated: Option<&[u8]>, imap: bool, smtp: bool) -> SharedProto {
    match negotiated {
        Some(p) if smtp && p == ALPN_SMTP.as_bytes() => SharedProto::Submission,
        Some(p) if imap && p == ALPN_IMAP.as_bytes() => SharedProto::Imap,
        _ => SharedProto::Http,
    }
}

/// TLS configs for the listeners that present the server certificate.
pub struct MailTlsConfigs {
    /// Implicit TLS on 993 — advertises only [`ALPN_IMAP`].
    pub imap_tls: Arc<ServerConfig>,
    /// Implicit TLS on 465 — advertises only [`ALPN_SMTP`].
    pub submission_tls: Arc<ServerConfig>,
    /// Advertises no ALPN at all. Used by STARTTLS upgrades on 143 / 587 and
    /// inbound SMTP on 25 — those negotiate TLS after a plaintext greeting, so the
    /// client has no ClientHello in which to offer ALPN — and by the HTTPS port
    /// when it is not multiplexing mail, which is how it behaves today.
    pub no_alpn: Arc<ServerConfig>,
}

/// Build the per-listener TLS configs from one certificate.
///
/// The dedicated implicit-TLS ports advertise exactly their own protocol, which
/// makes rustls answer a mismatched client with a fatal `no_application_protocol`
/// (RFC 9325 §3.8). That matters because these ports share the website's
/// certificate: TLS does not bind a connection to an intended port, so a lax mail
/// listener stays a valid substitute server for the web origin no matter how
/// strict the HTTPS port is — the cross-port residual ALPACA describes. Serving
/// each protocol under its own certificate is impossible here (one name, one
/// cert), leaving strict ALPN as the only TLS-layer defence.
///
/// A client that sends no ALPN extension is unaffected and still connects; rustls
/// only rejects when the client offered ALPN and nothing overlapped. Most desktop
/// mail clients send none, and Delta Chat omits it on the standard ports.
pub fn load_mail_tls_configs(cert: &Path, key: &Path) -> Result<MailTlsConfigs> {
    Ok(MailTlsConfigs {
        imap_tls: load_server_config_with_alpn(cert, key, &[ALPN_IMAP.as_bytes()])?,
        submission_tls: load_server_config_with_alpn(cert, key, &[ALPN_SMTP.as_bytes()])?,
        no_alpn: load_server_config(cert, key)?,
    })
}

/// Hostnames that identify a mail protocol when the client sent no ALPN.
///
/// `None` keeps the conventional prefix for that protocol (`imap.` / `smtp.`). A
/// configured name *replaces* the prefix rather than adding to it: an operator who
/// picks a neutral name does so precisely so that `imap.` is not a routable marker
/// on their server.
#[derive(Debug, Clone, Default)]
pub struct SniPolicy {
    pub imap: Option<String>,
    pub smtp: Option<String>,
}

/// Identify a protocol from the ClientHello's SNI. Only consulted when the client
/// offered no ALPN.
///
/// Safe in that position, and only in that position. Every browser sends ALPN — it
/// needs it for HTTP/2 — so a browser is decided at the ALPN step and can never
/// reach this one. Routing on SNI *first* would put `https://imap.example.org/`
/// into the IMAP parser with no attacker involved, which is the ALPACA-class
/// confusion this ordering exists to prevent.
pub fn route_sni(
    sni: Option<&str>,
    policy: &SniPolicy,
    imap: bool,
    smtp: bool,
) -> Option<SharedProto> {
    let host = sni?.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    if smtp && policy.smtp.as_deref().is_some_and(|n| eq_host(n, &host)) {
        return Some(SharedProto::Submission);
    }
    if imap && policy.imap.as_deref().is_some_and(|n| eq_host(n, &host)) {
        return Some(SharedProto::Imap);
    }
    match host.split('.').next()? {
        "smtp" | "submission" if smtp && policy.smtp.is_none() => Some(SharedProto::Submission),
        "imap" if imap && policy.imap.is_none() => Some(SharedProto::Imap),
        _ => None,
    }
}

fn eq_host(configured: &str, host: &str) -> bool {
    configured
        .trim()
        .trim_end_matches('.')
        .eq_ignore_ascii_case(host)
}

/// IMAP commands a client may legally send first (RFC 9051 §7.1.1 pre-auth state).
const IMAP_FIRST_COMMANDS: &[&str] = &[
    "CAPABILITY",
    "NOOP",
    "LOGOUT",
    "STARTTLS",
    "AUTHENTICATE",
    "LOGIN",
    "ID",
    "ENABLE",
];

/// HTTP methods, as the request line's first token (RFC 9110 §9).
const HTTP_METHODS: &[&str] = &[
    "GET", "POST", "PUT", "HEAD", "DELETE", "OPTIONS", "PATCH", "TRACE", "CONNECT",
];

/// Identify a protocol from the first bytes a client sent, or `None` if they say
/// nothing definite.
///
/// The last resort, reached only when the client offered neither ALPN nor a mail
/// hostname. It can only ever see a client that speaks *first*: IMAP and SMTP
/// servers are required to greet before the client may send a command, so a
/// conforming mail client sends nothing here and is identified by its silence
/// instead. What this does catch is an early-talking SMTP client (`EHLO`), an IMAP
/// client that pipelines its first tagged command, and any HTTP request line.
///
/// Deliberately conservative: bytes that match nothing return `None` rather than a
/// guess, because guessing wrong hands a stranger's connection to a mail parser.
pub fn classify_first_bytes(buf: &[u8]) -> Option<SharedProto> {
    // A request line or command fits well inside this; anything longer is not one.
    let head = buf.get(..buf.len().min(512))?;
    let line = std::str::from_utf8(head).ok()?;
    let line = line.split(['\r', '\n']).next()?.trim_end();
    let mut tokens = line.split(' ');
    let first = tokens.next()?;
    if first.is_empty() {
        return None;
    }

    if first.eq_ignore_ascii_case("EHLO") || first.eq_ignore_ascii_case("HELO") {
        return Some(SharedProto::Submission);
    }
    if HTTP_METHODS.contains(&first) {
        return Some(SharedProto::Http);
    }
    // IMAP is `<tag> <COMMAND> [args]`; the tag is client-chosen, so key off the
    // command and require the tag to look like one (RFC 9051 ABNF: no spaces).
    let command = tokens.next()?;
    if IMAP_FIRST_COMMANDS
        .iter()
        .any(|c| c.eq_ignore_ascii_case(command))
    {
        return Some(SharedProto::Imap);
    }
    None
}

/// Everything the mail branches need, so the listener signature stays readable.
pub struct SharedMail {
    pub ctx: Arc<AppState>,
    pub pool: DbPool,
    pub imap: ImapSessionConfig,
    pub submission: SmtpSessionConfig,
    /// Serve IMAP on this port for clients offering the [`ALPN_IMAP`] token.
    pub alpn_imap: bool,
    /// Serve submission on this port for clients offering the [`ALPN_SMTP`] token.
    pub alpn_smtp: bool,
    /// Hostnames that select a protocol when the client offers no ALPN.
    pub sni: SniPolicy,
}

/// How long to wait for an unidentified client to speak before serving it HTTPS.
///
/// Only connections that offered neither ALPN nor a mail hostname reach this, and
/// the wait is nearly free: an HTTP client sends its request line immediately, and
/// a silent one is handed to hyper, which would have been waiting for a request
/// anyway. It is a window for catching an early-talking mail client, not a guess.
const GREETING_PEEK: Duration = Duration::from_millis(250);

/// Accept loop for the HTTPS port when it also carries mail.
///
/// `tls` must advertise [`alpn_tokens`] for the same protocols as `mail`, otherwise
/// rustls will never negotiate them and every connection falls through to HTTP.
pub async fn run_shared_listener(
    addr: &str,
    cancel: CancellationToken,
    tls: Arc<ServerConfig>,
    router: Router,
    mail: SharedMail,
) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let mail = Arc::new(mail);
    info!(
        %addr,
        alpn_imap = mail.alpn_imap,
        alpn_smtp = mail.alpn_smtp,
        sni_imap = ?mail.sni.imap,
        sni_smtp = ?mail.sni.smtp,
        "shared TLS listener (HTTPS + mail)"
    );
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!(%addr, "shared listener stopped");
                break;
            }
            accept = listener.accept() => {
                let (stream, peer) = accept?;
                // Disable Nagle: IMAP is a chatty request/response protocol, and with
                // Nagle + delayed-ACK each small reply can stall 40–200ms.
                if let Err(e) = stream.set_nodelay(true) {
                    tracing::debug!(%peer, error = %e, "shared listener set_nodelay failed");
                }
                let tls = Arc::clone(&tls);
                let router = router.clone();
                let mail = Arc::clone(&mail);
                tokio::spawn(async move {
                    serve_one(stream, peer, tls, router, mail).await;
                });
            }
        }
    }
    Ok(())
}

/// Identify one connection and hand it to the right protocol handler.
async fn serve_one(
    stream: tokio::net::TcpStream,
    peer: std::net::SocketAddr,
    tls: Arc<ServerConfig>,
    router: Router,
    mail: Arc<SharedMail>,
) {
    // Read the ClientHello before choosing anything, so SNI is available even when
    // the client offered no ALPN for rustls to negotiate.
    let start = match LazyConfigAcceptor::new(rustls::server::Acceptor::default(), stream).await {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(%peer, error = %e, "shared TLS ClientHello failed");
            return;
        }
    };
    // The ClientHello borrows `start`, so decide inside this scope.
    let by_sni = {
        let hello = start.client_hello();
        if hello.alpn().is_some() {
            // ALPN decides, and rustls has already rejected a client whose offers
            // do not intersect ours. Never let a hostname override it: a browser
            // always sends ALPN, and its SNI is whatever is in the URL bar.
            None
        } else {
            route_sni(
                hello.server_name(),
                &mail.sni,
                mail.alpn_imap,
                mail.alpn_smtp,
            )
        }
    };

    let tls_stream = match start.into_stream(tls).await {
        Ok(s) => s,
        Err(e) => {
            // Also the path for a strict-ALPN rejection.
            tracing::debug!(%peer, error = %e, "shared TLS handshake failed");
            return;
        }
    };

    if let Some(proto) = by_sni {
        dispatch(proto, tls_stream, peer, router, mail).await;
        return;
    }

    let negotiated = tls_stream.get_ref().1.alpn_protocol().map(<[u8]>::to_vec);
    if negotiated.is_some() {
        let proto = route(negotiated.as_deref(), mail.alpn_imap, mail.alpn_smtp);
        dispatch(proto, tls_stream, peer, router, mail).await;
        return;
    }

    // Neither ALPN nor a mail hostname. Wait briefly for the client to speak: only
    // a client that speaks first can be identified here at all, since IMAP and SMTP
    // servers must greet before the client may send a command. `fill_buf` is
    // cancel-safe and leaves the bytes for the handler that follows.
    let mut buffered = BufReader::new(tls_stream);
    let proto = match tokio::time::timeout(GREETING_PEEK, buffered.fill_buf()).await {
        Ok(Ok(first)) => classify_first_bytes(first).unwrap_or(SharedProto::Http),
        Ok(Err(e)) => {
            tracing::debug!(%peer, error = %e, "shared listener read failed");
            return;
        }
        // Said nothing: unattributable, so serve it as HTTPS. Costs a silent client
        // nothing — hyper would be waiting for a request either way.
        Err(_) => SharedProto::Http,
    };
    dispatch(proto, buffered, peer, router, mail).await;
}

/// Run one identified connection as `proto`.
async fn dispatch<S>(
    proto: SharedProto,
    stream: S,
    peer: std::net::SocketAddr,
    router: Router,
    mail: Arc<SharedMail>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let result = match proto {
        SharedProto::Imap => {
            let _conn_guard = chatmail_metrics::conn_guard("imap");
            // Feed the same in-process counter the dedicated :993 listener does; the
            // `ss`-based fallback in admin status cannot see mail on a shared port.
            let peer_ip = peer.ip().to_string();
            connection_stats::on_open(&peer_ip);
            let mut session =
                ImapSession::new(Arc::clone(&mail.ctx), mail.pool.clone(), mail.imap.clone());
            let r = session.handle_tls_connection(stream).await;
            connection_stats::on_close(&peer_ip);
            r
        }
        SharedProto::Submission => {
            let _conn_guard = chatmail_metrics::conn_guard(mail.submission.module);
            let mut session = SmtpSession::new(
                Arc::clone(&mail.ctx),
                mail.pool.clone(),
                mail.submission.clone(),
            );
            session.handle_tls_connection(stream).await
        }
        SharedProto::Http => {
            chatmail_fed::serve_tls_conn(stream, router, peer).await;
            Ok(())
        }
    };
    if let Err(e) = result {
        tracing::debug!(%peer, ?proto, error = %e, "shared session ended");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chatmail_config::CredentialPolicy;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
    use rustls::{ClientConfig, RootCertStore};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    fn test_imap_cfg() -> ImapSessionConfig {
        ImapSessionConfig {
            hostname: "imap.test".into(),
            primary_domain: "test".into(),
            jit_domain: None,
            credential_policy: CredentialPolicy::default(),
            turn: None,
            iroh: None,
            push_enabled: false,
            starttls_config: None,
        }
    }

    fn test_submission_cfg() -> SmtpSessionConfig {
        SmtpSessionConfig {
            hostname: "smtp.test".into(),
            primary_domain: "test".into(),
            local_domains: vec!["test".into()],
            jit_domain: None,
            credential_policy: CredentialPolicy::default(),
            require_auth: true,
            module: "submission",
            starttls_config: None,
        }
    }

    struct Harness {
        addr: std::net::SocketAddr,
        roots: RootCertStore,
        cancel: CancellationToken,
        _dir: tempfile::TempDir,
    }

    impl Harness {
        async fn connect(
            &self,
            alpn: &[Vec<u8>],
        ) -> std::io::Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
            let mut cfg = ClientConfig::builder()
                .with_root_certificates(self.roots.clone())
                .with_no_client_auth();
            cfg.alpn_protocols = alpn.to_vec();
            let connector = tokio_rustls::TlsConnector::from(Arc::new(cfg));
            let tcp = tokio::net::TcpStream::connect(self.addr).await?;
            connector
                .connect(ServerName::try_from("localhost").unwrap(), tcp)
                .await
        }

        /// Connect with `alpn`, send `send`, return whatever comes back first.
        async fn probe(&self, alpn: &[Vec<u8>], send: &[u8]) -> String {
            let mut s = self.connect(alpn).await.expect("handshake");
            if !send.is_empty() {
                s.write_all(send).await.unwrap();
            }
            let mut buf = [0u8; 4096];
            let n = tokio::time::timeout(Duration::from_secs(5), s.read(&mut buf))
                .await
                .expect("no reply before timeout")
                .expect("read");
            String::from_utf8_lossy(&buf[..n]).into_owned()
        }
    }

    async fn harness() -> Harness {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let rc = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert = CertificateDer::from(rc.cert.der().to_vec());
        let key = PrivateKeyDer::Pkcs8(rc.key_pair.serialize_der().into());
        let mut server = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key)
            .unwrap();
        server.alpn_protocols = alpn_tokens(true, true);
        let mut roots = RootCertStore::empty();
        roots.add(cert).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        ctx.auth.hydrate(&pool).await.unwrap();

        let mail = SharedMail {
            ctx,
            pool,
            imap: test_imap_cfg(),
            submission: test_submission_cfg(),
            alpn_imap: true,
            alpn_smtp: true,
            sni: SniPolicy::default(),
        };

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let cancel = CancellationToken::new();
        let bg = cancel.clone();
        tokio::spawn(async move {
            let _ =
                run_shared_listener(&addr.to_string(), bg, Arc::new(server), Router::new(), mail)
                    .await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        Harness {
            addr,
            roots,
            cancel,
            _dir: dir,
        }
    }

    /// P12-UT05: the configured IMAP token routes to IMAP.
    #[test]
    fn p12_ut05_imap_token_routes_to_imap() {
        assert_eq!(route(Some(b"imap"), true, true), SharedProto::Imap);
    }

    /// P12-UT06: the configured SMTP token routes to submission.
    #[test]
    fn p12_ut06_smtp_token_routes_to_submission() {
        assert_eq!(route(Some(b"smtp"), true, true), SharedProto::Submission);
    }

    /// P12-UT07: a client that sends no ALPN gets HTTPS.
    ///
    /// Security-critical default. Routing an unattributable connection into a mail
    /// parser is the ALPACA hole (RFC 9325 §3.8); HTTP is the only safe assignment,
    /// and it matches upstream chatmail's nginx `default` branch.
    #[test]
    fn p12_ut07_absent_alpn_routes_to_http() {
        assert_eq!(route(None, true, true), SharedProto::Http);
    }

    /// P12-UT08: browser ALPN routes to HTTPS.
    #[test]
    fn p12_ut08_http_alpn_routes_to_http() {
        assert_eq!(route(Some(b"http/1.1"), true, true), SharedProto::Http);
    }

    /// P12-UT09: a protocol left unconfigured is never routed to, even if a client
    /// asks for it by name.
    #[test]
    fn p12_ut09_unconfigured_protocol_falls_back_to_http() {
        assert_eq!(route(Some(b"imap"), false, true), SharedProto::Http);
        assert_eq!(route(Some(b"smtp"), true, false), SharedProto::Http);
    }

    /// P12-UT10: advertised tokens are mail-first then `http/1.1`.
    ///
    /// rustls picks the first server token the client also offers, so this ordering
    /// reproduces upstream nginx's precedence (smtp before imap). `h2` is absent on
    /// purpose: advertising it would switch the whole HTTPS surface to HTTP/2.
    #[test]
    fn p12_ut10_advertised_token_order() {
        assert_eq!(
            alpn_tokens(true, true),
            vec![b"smtp".to_vec(), b"imap".to_vec(), b"http/1.1".to_vec()]
        );
        assert_eq!(
            alpn_tokens(true, false),
            vec![b"imap".to_vec(), b"http/1.1".to_vec()]
        );
    }

    /// P12-UT11: with neither mail protocol configured there is nothing to
    /// multiplex, so the port advertises no ALPN at all and behaves as it does today.
    #[test]
    fn p12_ut11_no_mail_protocols_advertises_nothing() {
        assert!(alpn_tokens(false, false).is_empty());
    }

    /// P12-IT01: one port, three protocols. ALPN `imap` reaches the IMAP greeting,
    /// ALPN `smtp` reaches the SMTP banner, and a client that offers no ALPN gets
    /// HTTP — all over the same TCP listener.
    #[tokio::test]
    async fn p12_it01_one_port_serves_imap_smtp_and_http() {
        let h = harness().await;

        assert!(
            h.probe(&[b"imap".to_vec()], b"").await.starts_with("* OK"),
            "ALPN imap must reach the IMAP greeting"
        );
        assert!(
            h.probe(&[b"smtp".to_vec()], b"").await.starts_with("220 "),
            "ALPN smtp must reach the SMTP banner"
        );
        let http = h.probe(&[], b"GET /nope HTTP/1.1\r\nHost: t\r\n\r\n").await;
        assert!(
            http.starts_with("HTTP/1.1"),
            "no ALPN must be served as HTTP, got: {http}"
        );

        h.cancel.cancel();
    }

    /// P12-IT02: a client offering only ALPN we do not serve is rejected at the
    /// handshake, not quietly handed to a protocol it did not ask for. This is the
    /// ALPACA countermeasure (RFC 9325 §3.8); without it the shared certificate
    /// makes the mail port a substitute server for the web origin.
    #[tokio::test]
    async fn p12_it02_unoffered_alpn_is_refused() {
        let h = harness().await;
        let err = h
            .connect(&[b"h2".to_vec()])
            .await
            .expect_err("h2 must fail");
        assert!(
            err.to_string().contains("alert") || err.to_string().contains("protocol"),
            "expected a no_application_protocol alert, got: {err}"
        );
        h.cancel.cancel();
    }

    /// P12-UT16: a config written by an older installer reads `alpn_smtp submission`.
    ///
    /// That value is a legacy maddy module-instance name, not an ALPN wire token —
    /// Delta Chat always offers the bare token `smtp`. Putting `submission` on the
    /// wire would make every such server reject real clients with a fatal
    /// no_application_protocol alert, so the directive only ever enables the
    /// protocol; the token comes from ALPN_SMTP.
    #[test]
    fn p12_ut16_legacy_directive_value_never_reaches_the_wire() {
        let legacy_config = Some("submission");
        let tokens = alpn_tokens(false, legacy_config.is_some());
        assert_eq!(tokens, vec![b"smtp".to_vec(), b"http/1.1".to_vec()]);
        assert_eq!(route(Some(b"smtp"), false, true), SharedProto::Submission);
        assert_eq!(route(Some(b"submission"), false, true), SharedProto::Http);
    }

    /// Spin up a dedicated implicit-TLS IMAP listener (993-style) whose TLS config
    /// advertises exactly `alpn`, and return its address plus a matching root store.
    async fn imap_only_listener(
        alpn: &[&[u8]],
    ) -> (std::net::SocketAddr, RootCertStore, CancellationToken) {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let rc = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert = CertificateDer::from(rc.cert.der().to_vec());
        let key = PrivateKeyDer::Pkcs8(rc.key_pair.serialize_der().into());
        let mut server = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key)
            .unwrap();
        server.alpn_protocols = alpn.iter().map(|p| p.to_vec()).collect();
        let mut roots = RootCertStore::empty();
        roots.add(cert).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        ctx.auth.hydrate(&pool).await.unwrap();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let cancel = CancellationToken::new();
        let bg = cancel.clone();
        tokio::spawn(async move {
            // Keep `dir` alive for the listener's lifetime.
            let _dir = dir;
            let _ = chatmail_imap::run_imap_listener(
                &addr.to_string(),
                bg,
                Some(Arc::new(server)),
                None,
                ctx,
                pool,
                ImapSessionConfig {
                    hostname: "imap.test".into(),
                    primary_domain: "test".into(),
                    jit_domain: None,
                    credential_policy: chatmail_config::CredentialPolicy::default(),
                    turn: None,
                    iroh: None,
                    push_enabled: false,
                    starttls_config: None,
                },
            )
            .await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        (addr, roots, cancel)
    }

    async fn tls_connect(
        addr: std::net::SocketAddr,
        roots: &RootCertStore,
        alpn: &[Vec<u8>],
    ) -> std::io::Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
        let mut cfg = ClientConfig::builder()
            .with_root_certificates(roots.clone())
            .with_no_client_auth();
        cfg.alpn_protocols = alpn.to_vec();
        let tcp = tokio::net::TcpStream::connect(addr).await?;
        tokio_rustls::TlsConnector::from(Arc::new(cfg))
            .connect(ServerName::try_from("localhost").unwrap(), tcp)
            .await
    }

    /// P12-IT04: restricting :993 to the `imap` ALPN must not lock out the clients
    /// that send no ALPN extension at all — which is most of them. Thunderbird and
    /// Apple Mail never send one, and Delta Chat omits it on the standard ports.
    ///
    /// rustls only sends `no_application_protocol` when the client offered ALPN and
    /// nothing overlapped; an absent extension negotiates nothing and connects. This
    /// test exists because that distinction is the whole safety argument for the
    /// change, and it is not obvious from the config.
    #[tokio::test]
    async fn p12_it04_alpn_less_client_still_reaches_imap() {
        let (addr, roots, cancel) = imap_only_listener(&[ALPN_IMAP.as_bytes()]).await;

        let mut s = tls_connect(addr, &roots, &[])
            .await
            .expect("a client sending no ALPN must still complete the handshake");
        assert!(
            s.get_ref().1.alpn_protocol().is_none(),
            "nothing should be negotiated when the client offers nothing"
        );

        let mut buf = [0u8; 512];
        let n = tokio::time::timeout(Duration::from_secs(5), s.read(&mut buf))
            .await
            .expect("greeting before timeout")
            .expect("read");
        assert!(
            String::from_utf8_lossy(&buf[..n]).starts_with("* OK"),
            "expected the IMAP greeting"
        );

        // A client that does offer the token still works.
        let s2 = tls_connect(addr, &roots, &[ALPN_IMAP.as_bytes().to_vec()])
            .await
            .expect("imap ALPN must be accepted");
        assert_eq!(s2.get_ref().1.alpn_protocol(), Some(ALPN_IMAP.as_bytes()));

        cancel.cancel();
    }

    /// P12-IT05: a browser pointed at the IMAP port is refused at the handshake.
    ///
    /// 993 presents the same certificate as the website, and TLS cannot bind a
    /// connection to an intended port, so without this a lax mail listener stays a
    /// valid substitute server for the web origin however strict :443 is — the
    /// cross-port residual ALPACA calls out. Certificate separation is unavailable
    /// by construction here, leaving strict ALPN as the only TLS-layer defence.
    #[tokio::test]
    async fn p12_it05_browser_alpn_refused_on_imap_port() {
        let (addr, roots, cancel) = imap_only_listener(&[ALPN_IMAP.as_bytes()]).await;

        let err = tls_connect(addr, &roots, &[b"h2".to_vec(), b"http/1.1".to_vec()])
            .await
            .expect_err("browser ALPN must be refused on the IMAP port");
        assert!(
            err.to_string().contains("alert") || err.to_string().contains("protocol"),
            "expected a no_application_protocol alert, got: {err}"
        );

        cancel.cancel();
    }

    /// P12-UT17: the dedicated implicit-TLS mail ports each advertise exactly their
    /// own protocol, and the config handed to STARTTLS upgrades advertises none.
    ///
    /// 143 and 587 negotiate TLS mid-session after a plaintext greeting, so the
    /// client has no ClientHello in which to offer ALPN; restricting that config
    /// would refuse every STARTTLS client.
    #[test]
    fn p12_ut17_dedicated_mail_ports_get_their_own_alpn() {
        let dir = tempfile::tempdir().unwrap();
        let rc = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert = dir.path().join("cert.pem");
        let key = dir.path().join("key.pem");
        std::fs::write(&cert, rc.cert.pem()).unwrap();
        std::fs::write(&key, rc.key_pair.serialize_pem()).unwrap();

        let tls = load_mail_tls_configs(&cert, &key).expect("configs must load");

        assert_eq!(tls.imap_tls.alpn_protocols, vec![b"imap".to_vec()]);
        assert_eq!(tls.submission_tls.alpn_protocols, vec![b"smtp".to_vec()]);
        assert!(
            tls.no_alpn.alpn_protocols.is_empty(),
            "STARTTLS upgrades carry no ClientHello to negotiate ALPN in"
        );
    }

    fn policy(imap: Option<&str>, smtp: Option<&str>) -> SniPolicy {
        SniPolicy {
            imap: imap.map(str::to_string),
            smtp: smtp.map(str::to_string),
        }
    }

    /// P12-UT18: with no ALPN, the conventional `imap.` / `smtp.` hostnames route to
    /// mail and everything else stays on HTTPS.
    #[test]
    fn p12_ut18_sni_prefix_routes_mail() {
        let p = policy(None, None);
        assert_eq!(
            route_sni(Some("imap.example.org"), &p, true, true),
            Some(SharedProto::Imap)
        );
        assert_eq!(
            route_sni(Some("smtp.example.org"), &p, true, true),
            Some(SharedProto::Submission)
        );
        assert_eq!(route_sni(Some("www.example.org"), &p, true, true), None);
        assert_eq!(route_sni(Some("example.org"), &p, true, true), None);
        assert_eq!(route_sni(None, &p, true, true), None);
    }

    /// P12-UT19: SNI comparison is case-insensitive and tolerates the trailing root dot.
    #[test]
    fn p12_ut19_sni_match_is_normalised() {
        let p = policy(None, None);
        assert_eq!(
            route_sni(Some("IMAP.Example.ORG."), &p, true, true),
            Some(SharedProto::Imap)
        );
    }

    /// P12-UT20: a configured hostname replaces the prefix default rather than adding
    /// to it. An operator picking a neutral name does so precisely so that `imap.`
    /// is not a routable marker on their server.
    #[test]
    fn p12_ut20_configured_sni_replaces_the_prefix_default() {
        let p = policy(Some("mail.example.org"), None);
        assert_eq!(
            route_sni(Some("mail.example.org"), &p, true, true),
            Some(SharedProto::Imap)
        );
        assert_eq!(
            route_sni(Some("imap.example.org"), &p, true, true),
            None,
            "configuring a name must retire the imap. prefix for that protocol"
        );
        // The protocol left unconfigured keeps its default.
        assert_eq!(
            route_sni(Some("smtp.example.org"), &p, true, true),
            Some(SharedProto::Submission)
        );
    }

    /// P12-UT21: a protocol that is switched off is never reachable by hostname.
    #[test]
    fn p12_ut21_sni_never_reaches_a_disabled_protocol() {
        let p = policy(Some("mail.example.org"), Some("send.example.org"));
        assert_eq!(route_sni(Some("mail.example.org"), &p, false, true), None);
        assert_eq!(
            route_sni(Some("imap.example.org"), &policy(None, None), false, true),
            None
        );
        assert_eq!(route_sni(Some("send.example.org"), &p, true, false), None);
    }

    /// P12-UT22: an early-talking client is identified by what it says first.
    ///
    /// Only reached when the client offered neither ALPN nor a mail hostname.
    #[test]
    fn p12_ut22_first_bytes_identify_the_protocol() {
        assert_eq!(
            classify_first_bytes(b"EHLO mx.example.org\r\n"),
            Some(SharedProto::Submission)
        );
        assert_eq!(
            classify_first_bytes(b"helo mx\r\n"),
            Some(SharedProto::Submission)
        );
        assert_eq!(
            classify_first_bytes(b"a001 CAPABILITY\r\n"),
            Some(SharedProto::Imap)
        );
        assert_eq!(
            classify_first_bytes(b". LOGIN user pass\r\n"),
            Some(SharedProto::Imap)
        );
        assert_eq!(
            classify_first_bytes(b"GET / HTTP/1.1\r\n"),
            Some(SharedProto::Http)
        );
        assert_eq!(
            classify_first_bytes(b"POST /mxdeliv HTTP/1.1\r\n"),
            Some(SharedProto::Http)
        );
    }

    /// P12-UT23: bytes that identify nothing identify nothing — no guessing.
    #[test]
    fn p12_ut23_unrecognised_first_bytes_are_not_guessed() {
        assert_eq!(classify_first_bytes(b""), None);
        assert_eq!(classify_first_bytes(b"\x16\x03\x01"), None);
        assert_eq!(classify_first_bytes(b"hello there\r\n"), None);
        // An HTTP-looking verb that is not one, and an IMAP tag with no known command.
        assert_eq!(classify_first_bytes(b"GETX / HTTP/1.1\r\n"), None);
        assert_eq!(classify_first_bytes(b"a001 FROBNICATE\r\n"), None);
    }

    /// Shared-port harness whose certificate also covers the mail hostnames, so a
    /// client can present them as SNI.
    async fn sni_harness() -> Harness {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let rc = rcgen::generate_simple_self_signed(vec![
            "localhost".into(),
            "imap.localhost".into(),
            "smtp.localhost".into(),
        ])
        .unwrap();
        let cert = CertificateDer::from(rc.cert.der().to_vec());
        let key = PrivateKeyDer::Pkcs8(rc.key_pair.serialize_der().into());
        let mut server = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key)
            .unwrap();
        server.alpn_protocols = alpn_tokens(true, true);
        let mut roots = RootCertStore::empty();
        roots.add(cert).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        ctx.auth.hydrate(&pool).await.unwrap();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let cancel = CancellationToken::new();
        let bg = cancel.clone();
        tokio::spawn(async move {
            let _ = run_shared_listener(
                &addr.to_string(),
                bg,
                Arc::new(server),
                Router::new(),
                SharedMail {
                    ctx,
                    pool,
                    imap: test_imap_cfg(),
                    submission: test_submission_cfg(),
                    alpn_imap: true,
                    alpn_smtp: true,
                    sni: SniPolicy::default(),
                },
            )
            .await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        Harness {
            addr,
            roots,
            cancel,
            _dir: dir,
        }
    }

    /// Connect presenting `host` as SNI and offering `alpn`, then return the first
    /// bytes the server sends after `send`.
    async fn probe_sni(h: &Harness, host: &str, alpn: &[Vec<u8>], send: &[u8]) -> String {
        let mut cfg = ClientConfig::builder()
            .with_root_certificates(h.roots.clone())
            .with_no_client_auth();
        cfg.alpn_protocols = alpn.to_vec();
        let tcp = tokio::net::TcpStream::connect(h.addr).await.unwrap();
        let mut s = tokio_rustls::TlsConnector::from(Arc::new(cfg))
            .connect(ServerName::try_from(host.to_string()).unwrap(), tcp)
            .await
            .expect("handshake");
        if !send.is_empty() {
            s.write_all(send).await.unwrap();
        }
        let mut buf = [0u8; 4096];
        let n = tokio::time::timeout(Duration::from_secs(5), s.read(&mut buf))
            .await
            .expect("reply before timeout")
            .expect("read");
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }

    /// P12-IT06: a client that sends no ALPN is identified by the hostname it asked
    /// for. This is what lets a stock mail client — Thunderbird, Apple Mail, K-9,
    /// none of which send ALPN — reach mail on the shared port.
    #[tokio::test]
    async fn p12_it06_sni_identifies_mail_without_alpn() {
        let h = sni_harness().await;

        assert!(
            probe_sni(&h, "imap.localhost", &[], b"")
                .await
                .starts_with("* OK"),
            "imap. hostname must reach IMAP"
        );
        assert!(
            probe_sni(&h, "smtp.localhost", &[], b"")
                .await
                .starts_with("220 "),
            "smtp. hostname must reach submission"
        );

        h.cancel.cancel();
    }

    /// P12-IT07: ALPN outranks SNI.
    ///
    /// This ordering is the security property, not a preference. Browsers always
    /// send ALPN, so deciding on ALPN first means a browser can never be steered
    /// into a mail parser by the hostname in its own URL bar.
    #[tokio::test]
    async fn p12_it07_alpn_outranks_sni() {
        let h = sni_harness().await;

        let reply = probe_sni(
            &h,
            "imap.localhost",
            &[b"http/1.1".to_vec()],
            b"GET / HTTP/1.1\r\nHost: imap.localhost\r\n\r\n",
        )
        .await;
        assert!(
            reply.starts_with("HTTP/1.1"),
            "a browser asking for http/1.1 at imap.localhost must get HTTP, got: {reply}"
        );

        h.cancel.cancel();
    }

    /// P12-IT08: with neither ALPN nor a mail hostname, an early-talking client is
    /// identified by what it says first — and anything unrecognised is served as
    /// HTTPS rather than guessed into a mail parser.
    #[tokio::test]
    async fn p12_it08_first_bytes_identify_when_nothing_else_does() {
        let h = sni_harness().await;

        let smtp = probe_sni(&h, "localhost", &[], b"EHLO client.example\r\n").await;
        assert!(
            smtp.starts_with("220 "),
            "an early EHLO must reach submission, got: {smtp}"
        );

        let http = probe_sni(&h, "localhost", &[], b"GET / HTTP/1.1\r\nHost: t\r\n\r\n").await;
        assert!(
            http.starts_with("HTTP/1.1"),
            "an HTTP request line must reach HTTPS, got: {http}"
        );

        h.cancel.cancel();
    }
}
