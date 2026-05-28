//! TCP IMAP client for integration tests (Delta Chat / relay-ping style dialog).

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Minimal PGP/MIME body accepted by chatmail PGP policy (TDD `03-imap-server.md` APPEND).
pub const PGP_MIME_BODY: &[u8] = b"From: u@test\r\nTo: u@test\r\nSubject: sync\r\n\
Content-Type: multipart/encrypted; boundary=\"b\"\r\n\r\n\
--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n\
--b\r\nContent-Type: application/octet-stream\r\n\r\nstub\r\n--b--\r\n";

pub fn pgp_mime_for_user(user: &str) -> Vec<u8> {
    std::str::from_utf8(PGP_MIME_BODY)
        .unwrap()
        .replace("u@test", user)
        .into_bytes()
}

/// IMAP4rev1 client over plain TCP (dev port 1143).
pub struct ImapClient {
    stream: TcpStream,
    transcript: String,
}

impl ImapClient {
    pub async fn connect(addr: std::net::SocketAddr) -> Self {
        let mut stream = TcpStream::connect(addr).await.expect("imap connect");
        let greeting = Self::read_until_inner(&mut stream, "ready", Duration::from_secs(3)).await;
        Self {
            stream,
            transcript: greeting,
        }
    }

    pub fn transcript(&self) -> &str {
        &self.transcript
    }

    pub async fn send_line(&mut self, line: &str) {
        self.stream
            .write_all(line.as_bytes())
            .await
            .expect("imap write");
        self.stream
            .write_all(b"\r\n")
            .await
            .expect("imap crlf");
    }

    /// Send a command and read until a tagged completion, BAD, or NO […].
    pub async fn command(&mut self, line: &str) -> String {
        self.send_line(line).await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        let chunk = Self::read_until_command_done(&mut self.stream).await;
        self.transcript.push_str(&chunk);
        chunk
    }

    /// APPEND with `{n}` literal on the same line (RFC 3501).
    pub async fn append_literal(&mut self, tag_line: &str, body: &[u8]) -> String {
        self.send_line(tag_line).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        self.stream.write_all(body).await.expect("literal");
        self.stream.write_all(b"\r\n").await.expect("literal crlf");
        let chunk = Self::read_until_command_done(&mut self.stream).await;
        self.transcript.push_str(&chunk);
        chunk
    }

    /// Start IDLE; returns after server `+ idling`.
    pub async fn idle_start(&mut self, tag: &str) -> String {
        self.send_line(&format!("{tag} IDLE")).await;
        let chunk =
            Self::read_until_inner(&mut self.stream, "+ idling", Duration::from_secs(3)).await;
        self.transcript.push_str(&chunk);
        chunk
    }

    /// Read until `needle` appears (unsolicited EXISTS during IDLE).
    pub async fn read_until(&mut self, needle: &str, timeout: Duration) -> String {
        let chunk = Self::read_until_inner(&mut self.stream, needle, timeout).await;
        self.transcript.push_str(&chunk);
        chunk
    }

    /// End IDLE with DONE (untagged or `tag DONE`).
    pub async fn idle_done(&mut self, tag: &str) -> String {
        self.send_line("DONE").await;
        let chunk = Self::read_until_inner(
            &mut self.stream,
            &format!("{tag} OK"),
            Duration::from_secs(3),
        )
        .await;
        self.transcript.push_str(&chunk);
        chunk
    }

    async fn read_until_command_done(stream: &mut TcpStream) -> String {
        Self::read_until_inner(stream, "__cmd_done__", Duration::from_secs(3)).await
    }

    async fn read_until_inner(stream: &mut TcpStream, needle: &str, timeout: Duration) -> String {
        let mut buf = [0u8; 65536];
        tokio::time::timeout(timeout, async {
            let mut acc = String::new();
            for _ in 0..100 {
                let n = stream.read(&mut buf).await.unwrap_or(0);
                if n > 0 {
                    acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if needle == "__cmd_done__" {
                        if acc.contains(" completed\r\n")
                            || acc.contains(" completed\n")
                            || acc.contains("NO ")
                            || acc.contains("BAD ")
                        {
                            return acc;
                        }
                    } else if acc.contains(needle) {
                        return acc;
                    }
                }
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
            acc
        })
        .await
        .unwrap_or_default()
    }
}
