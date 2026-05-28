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

//! Runtime settings keys stored in the `settings` table (Madmail-compatible).
//!
//! See `context/madmail/internal/api/admin/resources/settings.go` and
//! `context/madmail/docs/chatmail/settings_db.md`.

// ── Toggle settings ──────────────────────────────────────────────────────────
pub const REGISTRATION_OPEN: &str = "__REGISTRATION_OPEN__";
pub const JIT_REGISTRATION_ENABLED: &str = "__JIT_REGISTRATION_ENABLED__";
pub const REGISTRATION_TOKEN_REQUIRED: &str = "__REGISTRATION_TOKEN_REQUIRED__";
pub const TURN_ENABLED: &str = "__TURN_ENABLED__";
pub const IROH_ENABLED: &str = "__IROH_ENABLED__";
pub const SS_ENABLED: &str = "__SS_ENABLED__";
pub const SS_WS_ENABLED: &str = "__SS_WS_ENABLED__";
pub const SS_GRPC_ENABLED: &str = "__SS_GRPC_ENABLED__";
/// Auto-delete seen IMAP messages when enabled (`/admin/services/auto_purge_seen`).
pub const AUTO_PURGE_SEEN: &str = "__AUTO_PURGE_SEEN__";
/// Hourly deletion of message files older than `__MESSAGE_RETENTION__`.
pub const MESSAGE_RETENTION_ENABLED: &str = "__MESSAGE_RETENTION_ENABLED__";
/// Go-style duration (e.g. `30d`, `720h`) for maildir file rotation.
pub const MESSAGE_RETENTION: &str = "__MESSAGE_RETENTION__";
pub const HTTP_PROXY_ENABLED: &str = "__HTTP_PROXY_ENABLED__";
pub const ADMIN_WEB_ENABLED: &str = "__ADMIN_WEB_ENABLED__";
pub const WEBIMAP_ENABLED: &str = "__WEBIMAP_ENABLED__";
pub const WEBSMTP_ENABLED: &str = "__WEBSMTP_ENABLED__";
pub const FEDERATION_POLICY: &str = "__FEDERATION_POLICY__";
pub const FEDERATION_ENABLED: &str = "__FEDERATION_ENABLED__";

// ── Port settings ────────────────────────────────────────────────────────────
pub const SMTP_PORT: &str = "__SMTP_PORT__";
pub const SUBMISSION_PORT: &str = "__SUBMISSION_PORT__";
pub const SUBMISSION_TLS_PORT: &str = "__SUBMISSION_TLS_PORT__";
pub const IMAP_PORT: &str = "__IMAP_PORT__";
pub const IMAP_TLS_PORT: &str = "__IMAP_TLS_PORT__";
pub const TURN_PORT: &str = "__TURN_PORT__";
pub const SASL_PORT: &str = "__SASL_PORT__";
pub const IROH_PORT: &str = "__IROH_PORT__";
pub const SS_PORT: &str = "__SS_PORT__";
pub const SS_WS_PORT: &str = "__SS_WS_PORT__";
pub const SS_GRPC_PORT: &str = "__SS_GRPC_PORT__";
pub const HTTP_PORT: &str = "__HTTP_PORT__";
pub const HTTPS_PORT: &str = "__HTTPS_PORT__";
pub const HTTP_PROXY_PORT: &str = "__HTTP_PROXY_PORT__";

// ── Per-port access (local only when "true") ─────────────────────────────────
pub const SMTP_LOCAL_ONLY: &str = "__SMTP_LOCAL_ONLY__";
pub const SUBMISSION_LOCAL_ONLY: &str = "__SUBMISSION_LOCAL_ONLY__";
pub const SUBMISSION_TLS_LOCAL_ONLY: &str = "__SUBMISSION_TLS_LOCAL_ONLY__";
pub const IMAP_LOCAL_ONLY: &str = "__IMAP_LOCAL_ONLY__";
pub const IMAP_TLS_LOCAL_ONLY: &str = "__IMAP_TLS_LOCAL_ONLY__";
pub const TURN_LOCAL_ONLY: &str = "__TURN_LOCAL_ONLY__";
pub const SASL_LOCAL_ONLY: &str = "__SASL_LOCAL_ONLY__";
pub const IROH_LOCAL_ONLY: &str = "__IROH_LOCAL_ONLY__";
pub const HTTP_LOCAL_ONLY: &str = "__HTTP_LOCAL_ONLY__";
pub const HTTPS_LOCAL_ONLY: &str = "__HTTPS_LOCAL_ONLY__";

// ── Configuration settings ───────────────────────────────────────────────────
pub const SMTP_HOSTNAME: &str = "__SMTP_HOSTNAME__";
pub const TURN_REALM: &str = "__TURN_REALM__";
pub const TURN_SECRET: &str = "__TURN_SECRET__";
pub const TURN_RELAY_IP: &str = "__TURN_RELAY_IP__";
pub const TURN_TTL: &str = "__TURN_TTL__";
pub const IROH_RELAY_URL: &str = "__IROH_RELAY_URL__";
pub const SS_CIPHER: &str = "__SS_CIPHER__";
pub const SS_PASSWORD: &str = "__SS_PASSWORD__";
pub const HTTP_PROXY_PATH: &str = "__HTTP_PROXY_PATH__";
pub const HTTP_PROXY_USERNAME: &str = "__HTTP_PROXY_USERNAME__";
pub const HTTP_PROXY_PASSWORD: &str = "__HTTP_PROXY_PASSWORD__";
pub const ADMIN_PATH: &str = "__ADMIN_PATH__";
pub const ADMIN_WEB_PATH: &str = "__ADMIN_WEB_PATH__";
pub const DCLOGIN_IMAP_SECURITY: &str = "__DCLOGIN_IMAP_SECURITY__";
pub const DCLOGIN_SMTP_SECURITY: &str = "__DCLOGIN_SMTP_SECURITY__";
pub const LANGUAGE: &str = "__LANGUAGE__";
/// `storage.imapsql` `appendlimit` override (e.g. `100M`).
pub const APPENDLIMIT: &str = "__APPENDLIMIT__";
/// `smtp` / `submission` `max_message_size` override (e.g. `100M`).
pub const MAX_MESSAGE_SIZE: &str = "__MAX_MESSAGE_SIZE__";

/// Pseudo-username row in `quotas` for server-wide default cap.
pub const GLOBAL_QUOTA_USERNAME: &str = "__GLOBAL_DEFAULT__";
