// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chatmail_types::Result;
use qrcode::render::unicode;
use qrcode::{EcLevel, QrCode};
use serde_json::json;

/// Hosted admin panel used in login QR codes (scan → open PWA with credentials).
pub const DEFAULT_ADMIN_WEB_PANEL: &str = "https://admin.madmail.chat";

fn encode_login_hash(api_url: &str, token: &str) -> String {
    let payload = json!({ "u": api_url, "t": token }).to_string();
    URL_SAFE_NO_PAD.encode(payload.as_bytes())
}

/// Compact QR payload for terminal scanning (admin web accepts raw `#` hash bodies).
pub fn login_qr_scan_payload(api_url: &str, token: &str) -> String {
    encode_login_hash(api_url, token)
}

/// Build URL for the admin web login QR (`/#<base64-json>`).
pub fn build_admin_login_qr_url(api_url: &str, token: &str) -> String {
    format!(
        "{DEFAULT_ADMIN_WEB_PANEL}/#{}",
        encode_login_hash(api_url, token)
    )
}

/// Print a scannable QR code to the terminal (Unicode block rendering).
pub fn print_login_qr_terminal(data: &str) -> Result<()> {
    // High error correction helps when scanning dense URLs from a terminal display.
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::H)
        .map_err(|e| chatmail_types::ChatmailError::config(format!("QR encode: {e}")))?;
    let image = code
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Dark)
        .light_color(unicode::Dense1x2::Light)
        .build();
    println!("{image}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_qr_url_uses_hash_fragment() {
        let url = build_admin_login_qr_url("https://1.1.1.1/api/admin", "abc123");
        assert!(url.starts_with("https://admin.madmail.chat/#"));
        assert!(!url.contains('?'));
        assert!(!url.contains("madmail_u"));
    }

    #[test]
    fn login_qr_scan_payload_is_shorter_than_full_url() {
        let api = "https://1.1.1.1/api/admin";
        let token = "abc123";
        let url = build_admin_login_qr_url(api, token);
        let payload = login_qr_scan_payload(api, token);
        assert!(url.ends_with(&payload));
        assert!(payload.len() < url.len());
    }
}
