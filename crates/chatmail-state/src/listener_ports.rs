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

//! Effective TCP ports bound by the running supervisor (for `/admin/status` and dclogin).

use std::sync::RwLock;

/// Bind addresses actually listening (updated on boot and soft reload).
#[derive(Debug, Clone, Default)]
pub struct ListenerPorts {
    pub imap_plain_port: String,
    pub imap_tls_port: String,
    pub imap_plain_addr: Option<String>,
    pub imap_tls_addr: Option<String>,
    pub smtp_addr: Option<String>,
    pub submission_plain_addr: Option<String>,
    pub submission_tls_addr: Option<String>,
    pub submission_plain_port: String,
    pub submission_tls_port: String,
    pub http_plain_addr: Option<String>,
    pub http_tls_addr: Option<String>,
    pub http_plain_port: String,
    pub http_tls_port: String,
}

#[derive(Debug, Default)]
pub struct ListenerPortsStore(RwLock<ListenerPorts>);

fn port_from_addr(addr: Option<&String>) -> String {
    addr.and_then(|a| a.rsplit_once(':').map(|(_, p)| p.to_string()))
        .unwrap_or_default()
}

impl ListenerPortsStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_runtime(
        &self,
        smtp_addr: impl Into<String>,
        imap_plain: Option<String>,
        imap_tls: Option<String>,
        submission_plain: Option<String>,
        submission_tls: Option<String>,
        http_plain: Option<String>,
        http_tls: Option<String>,
    ) {
        let smtp_addr = smtp_addr.into();
        if let Ok(mut g) = self.0.write() {
            g.smtp_addr = Some(smtp_addr);
            g.imap_plain_addr = imap_plain.clone();
            g.imap_tls_addr = imap_tls.clone();
            g.submission_plain_addr = submission_plain.clone();
            g.submission_tls_addr = submission_tls.clone();
            g.http_plain_addr = http_plain.clone();
            g.http_tls_addr = http_tls.clone();
            g.imap_plain_port = port_from_addr(imap_plain.as_ref());
            g.imap_tls_port = port_from_addr(imap_tls.as_ref());
            g.submission_plain_port = port_from_addr(submission_plain.as_ref());
            g.submission_tls_port = port_from_addr(submission_tls.as_ref());
            g.http_plain_port = port_from_addr(http_plain.as_ref());
            g.http_tls_port = port_from_addr(http_tls.as_ref());
        }
    }

    /// Primary plain IMAP port for admin status (falls back to TLS port).
    pub fn imap_port(&self) -> String {
        self.0
            .read()
            .map(|g| {
                if !g.imap_plain_port.is_empty() {
                    g.imap_plain_port.clone()
                } else {
                    g.imap_tls_port.clone()
                }
            })
            .unwrap_or_default()
    }

    pub fn snapshot(&self) -> ListenerPorts {
        self.0.read().map(|g| g.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_runtime_populates_ports_and_addrs() {
        let store = ListenerPortsStore::new();
        store.set_runtime(
            "0.0.0.0:25",
            Some("0.0.0.0:143".into()),
            Some("0.0.0.0:993".into()),
            Some("0.0.0.0:587".into()),
            Some("0.0.0.0:465".into()),
            Some("127.0.0.1:80".into()),
            Some("0.0.0.0:443".into()),
        );
        let snap = store.snapshot();
        assert_eq!(snap.smtp_addr.as_deref(), Some("0.0.0.0:25"));
        assert_eq!(snap.imap_plain_port, "143");
        assert_eq!(snap.imap_tls_port, "993");
        assert_eq!(snap.submission_plain_port, "587");
        assert_eq!(snap.submission_tls_port, "465");
        assert_eq!(snap.http_plain_port, "80");
        assert_eq!(snap.http_tls_port, "443");
        assert_eq!(store.imap_port(), "143");
    }

    #[test]
    fn imap_port_falls_back_to_tls_when_plain_unset() {
        let store = ListenerPortsStore::new();
        store.set_runtime(
            "0.0.0.0:25",
            None,
            Some("0.0.0.0:993".into()),
            None,
            None,
            None,
            None,
        );
        assert_eq!(store.imap_port(), "993");
        let snap = store.snapshot();
        assert!(snap.imap_plain_port.is_empty());
        assert_eq!(snap.imap_tls_port, "993");
    }

    #[test]
    fn snapshot_default_when_never_configured() {
        let store = ListenerPortsStore::new();
        let snap = store.snapshot();
        assert!(snap.smtp_addr.is_none());
        assert!(snap.imap_plain_port.is_empty());
        assert_eq!(store.imap_port(), "");
    }
}
