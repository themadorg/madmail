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

//! Server TLS from PEM files (`tls file` in maddy.conf).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use chatmail_types::{ChatmailError, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};

pub fn load_server_config(cert_path: &Path, key_path: &Path) -> Result<Arc<ServerConfig>> {
    let certs = load_certs(cert_path)?;
    let key = load_private_key(key_path)?;
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| ChatmailError::config(format!("TLS server config: {e}")))?;
    Ok(Arc::new(config))
}

fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let file = File::open(path)
        .map_err(|e| ChatmailError::config(format!("open TLS certificate {}: {e}", path.display())))?;
    let mut reader = BufReader::new(file);
    let mut out = Vec::new();
    for item in certs(&mut reader) {
        let der = item.map_err(|e| ChatmailError::config(format!("parse TLS certificate: {e}")))?;
        out.push(der);
    }
    if out.is_empty() {
        return Err(ChatmailError::config(format!(
            "no certificates in {}",
            path.display()
        )));
    }
    Ok(out)
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    let file = File::open(path)
        .map_err(|e| ChatmailError::config(format!("open TLS private key {}: {e}", path.display())))?;
    let mut reader = BufReader::new(file);
    if let Some(key) = pkcs8_private_keys(&mut reader).filter_map(|r| r.ok()).next() {
        return Ok(PrivateKeyDer::Pkcs8(key));
    }
    let file = File::open(path)
        .map_err(|e| ChatmailError::config(format!("open TLS private key {}: {e}", path.display())))?;
    let mut reader = BufReader::new(file);
    let key = rsa_private_keys(&mut reader)
        .filter_map(|r| r.ok())
        .next()
        .ok_or_else(|| ChatmailError::config(format!("no private key in {}", path.display())))?;
    Ok(PrivateKeyDer::Pkcs1(key))
}
