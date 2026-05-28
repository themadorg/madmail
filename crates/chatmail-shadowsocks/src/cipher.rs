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

use shadowsocks::crypto::CipherKind;

/// Map `maddy.conf` / Madmail cipher name to shadowsocks-rs `CipherKind`.
pub fn parse_cipher(name: &str) -> Option<CipherKind> {
    match name.trim().to_ascii_lowercase().as_str() {
        "aes-128-gcm" | "aes_128_gcm" => Some(CipherKind::AES_128_GCM),
        "aes-256-gcm" | "aes_256_gcm" => Some(CipherKind::AES_256_GCM),
        "chacha20-ietf-poly1305" | "chacha20_ietf_poly1305" => Some(CipherKind::CHACHA20_POLY1305),
        _ => None,
    }
}

/// Xray JSON `method` field (lowercase hyphenated).
pub(crate) fn xray_method(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "aes-128-gcm" | "aes_128_gcm" => Some("aes-128-gcm"),
        "aes-256-gcm" | "aes_256_gcm" => Some("aes-256-gcm"),
        "chacha20-ietf-poly1305" | "chacha20_ietf_poly1305" => Some("chacha20-ietf-poly1305"),
        _ => None,
    }
}
