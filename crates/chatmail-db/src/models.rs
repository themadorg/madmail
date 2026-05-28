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

//! SQLite row types for Phase 1 tables (quotas, blocklist, etc.).

/// Per-user storage quota (`quotas` table).
#[derive(Debug, Clone)]
pub struct Quota {
    pub username: String,
    pub max_storage: i64,
    pub created_at: i64,
    pub first_login_at: i64,
    pub last_login_at: i64,
    pub used_token: Option<String>,
}

/// Blocked username (`blocked_users` table).
#[derive(Debug, Clone)]
pub struct BlockedUser {
    pub username: String,
    pub reason: String,
}

/// DNS / endpoint override (`dns_overrides` table).
#[derive(Debug, Clone)]
pub struct DnsOverride {
    pub lookup_key: String,
    pub target_host: String,
    pub comment: Option<String>,
}

/// Registration token (`registration_tokens` table).
#[derive(Debug, Clone)]
pub struct RegistrationToken {
    pub token: String,
    pub max_uses: i32,
    pub used_count: i32,
    pub comment: Option<String>,
}
