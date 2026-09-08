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

//! One TLS port serving HTTPS, IMAP and submission, selected by ALPN.
//!
//! This module also owns the ALPN token policy for the *dedicated* mail ports —
//! see [`load_mail_tls_configs`] — so every listener presenting the shared
//! certificate agrees on which protocols it will speak.
//!
//! Upstream chatmail does this in nginx's `stream` module with `ssl_preread`,
//! passing raw TCP through to Dovecot/Postfix. Madmail is a single binary with no
//! proxy in front, so it terminates TLS once and dispatches on the protocol rustls
//! negotiated. Terminating here also lets rustls enforce ALPN strictly (a client
//! whose offers do not intersect ours gets a fatal `no_application_protocol`),
//! which the passthrough design cannot do — see RFC 9325 §3.8 and the ALPACA
//! cross-protocol attack.
//!
//! Routing is by ALPN only. Never by SNI: SNI names a host, not a protocol, and a
//! browser controls it from the URL bar, so `https://imap.example.org/` would land
//! in the IMAP parser with no attacker involved. There is also no byte-sniffing
//! fallback — IMAP and SMTP are server-speaks-first while HTTP is
//! client-speaks-first, so no first byte can distinguish them without hanging one
//! side. A connection with no ALPN is therefore served as HTTPS, matching
//! upstream's nginx `default` branch.

use std::path::Path;
use std::sync::Arc;

use axum::Router;
use chatmail_db::DbPool;
use chatmail_imap::{connection_stats, ImapSession, ImapSessionConfig};
use chatmail_smtp::{SmtpSession, SmtpSessionConfig};
use chatmail_state::AppState;
use chatmail_tls::{load_server_config, load_server_config_with_alpn};
use chatmail_types::Result;
use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
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
}

/// Accept loop for the HTTPS port when it also carries mail.
///
/// `tls` must advertise [`alpn_tokens`] for the same tokens as `mail`, otherwise
/// rustls will never negotiate them and every connection falls through to HTTP.
pub async fn run_shared_listener(
    addr: &str,
    cancel: CancellationToken,
    tls: Arc<ServerConfig>,
    router: Router,
    mail: SharedMail,
) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let acceptor = TlsAcceptor::from(tls);
    let mail = Arc::new(mail);
    info!(
        %addr,
        alpn_imap = mail.alpn_imap,
        alpn_smtp = mail.alpn_smtp,
        "shared TLS listener (HTTPS + ALPN mail)"
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
                let acceptor = acceptor.clone();
                let router = router.clone();
                let mail = Arc::clone(&mail);
                tokio::spawn(async move {
                    let tls_stream = match acceptor.accept(stream).await {
                        Ok(s) => s,
                        Err(e) => {
                            // Also the path for a strict-ALPN rejection.
                            tracing::debug!(%peer, error = %e, "shared TLS handshake failed");
                            return;
                        }
                    };
                    let proto = route(
                        tls_stream.get_ref().1.alpn_protocol(),
                        mail.alpn_imap,
                        mail.alpn_smtp,
                    );
                    let result = match proto {
                        SharedProto::Imap => {
                            let _conn_guard = chatmail_metrics::conn_guard("imap");
                            // Feed the same in-process counter the dedicated :993
                            // listener does; the `ss`-based fallback in admin status
                            // cannot see mail connections on a shared port.
                            let peer_ip = peer.ip().to_string();
                            connection_stats::on_open(&peer_ip);
                            let mut session = ImapSession::new(
                                Arc::clone(&mail.ctx),
                                mail.pool.clone(),
                                mail.imap.clone(),
                            );
                            let r = session.handle_tls_connection(tls_stream).await;
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
                            session.handle_tls_connection(tls_stream).await
                        }
                        SharedProto::Http => {
                            chatmail_fed::serve_tls_conn(tls_stream, router, peer).await;
                            Ok(())
                        }
                    };
                    if let Err(e) = result {
                        tracing::debug!(%peer, ?proto, error = %e, "shared session ended");
                    }
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chatmail_config::CredentialPolicy;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
    use rustls::{ClientConfig, RootCertStore};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

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
            imap: ImapSessionConfig {
                hostname: "imap.test".into(),
                primary_domain: "test".into(),
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                turn: None,
                iroh: None,
                push_enabled: false,
                starttls_config: None,
            },
            submission: SmtpSessionConfig {
                hostname: "smtp.test".into(),
                primary_domain: "test".into(),
                local_domains: vec!["test".into()],
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                require_auth: true,
                module: "submission",
                starttls_config: None,
            },
            alpn_imap: true,
            alpn_smtp: true,
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
}
