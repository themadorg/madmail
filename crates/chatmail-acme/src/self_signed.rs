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

//! Self-signed TLS certificates (Madmail `tls-mode self_signed`).

use std::path::Path;

use chatmail_types::{ChatmailError, Result};
use rcgen::generate_simple_self_signed;

/// Generate and write a self-signed cert/key pair (~1 year via rcgen defaults).
pub fn generate_self_signed(
    primary_domain: &str,
    hostname: &str,
    _public_ip: &str,
    cert_path: &Path,
    key_path: &Path,
) -> Result<()> {
    let mut names = vec![
        primary_domain.trim_matches(|c| c == '[' || c == ']').to_string(),
        hostname.trim_matches(|c| c == '[' || c == ']').to_string(),
    ];
    names.sort();
    names.dedup();

    let cert = generate_simple_self_signed(names).map_err(|e| ChatmailError::config(e.to_string()))?;

    write_pem_pair(
        cert_path,
        key_path,
        cert.cert.pem().as_bytes(),
        cert.key_pair.serialize_pem().as_bytes(),
    )
}

fn write_pem_pair(cert_path: &Path, key_path: &Path, cert_pem: &[u8], key_pem: &[u8]) -> Result<()> {
    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ChatmailError::config(format!("create {}: {e}", parent.display())))?;
    }
    std::fs::write(cert_path, cert_pem)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", cert_path.display())))?;
    std::fs::write(key_path, key_pem)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", key_path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(cert_path, std::fs::Permissions::from_mode(0o640)).ok();
        std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600)).ok();
    }
    Ok(())
}
