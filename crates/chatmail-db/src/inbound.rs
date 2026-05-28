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

//! Inbound delivery guards (SMTP :25, `/mxdeliv`, local queue final delivery).

use chatmail_types::Result;
use crate::DbPool;

use crate::passwords;

const BLOCKED_RCPT_LOCAL_PARTS: &[&str] = &[
    "admin",
    "root",
    "postmaster",
    "mailer-daemon",
    "abuse",
    "hostmaster",
    "webmaster",
];

/// Federation must not deliver to reserved local parts (`admin@`, `postmaster@`, …).
pub fn is_federation_rcpt_blocked(rcpt: &str) -> bool {
    let Some((local, _)) = rcpt.rsplit_once('@') else {
        return true;
    };
    let local = local.to_ascii_lowercase();
    BLOCKED_RCPT_LOCAL_PARTS.contains(&local.as_str())
}

/// Inbound SMTP / `/mxdeliv`: drop mail from `admin@…` (admin notices use a separate API).
pub fn is_federation_sender_blocked(mail_from: &str) -> bool {
    let Some((local, _)) = mail_from.rsplit_once('@') else {
        return false;
    };
    local.eq_ignore_ascii_case("admin")
}

/// Whether a local recipient may receive inbound mail (account exists, not a reserved address).
pub async fn inbound_local_recipient_allowed(pool: &DbPool, rcpt: &str) -> Result<bool> {
    if is_federation_rcpt_blocked(rcpt) {
        return Ok(false);
    }
    passwords::user_exists(pool, rcpt).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_memory_db;

    #[test]
    fn blocks_admin_sender() {
        assert!(is_federation_sender_blocked("admin@example.org"));
        assert!(is_federation_sender_blocked("Admin@1.2.3.4"));
        assert!(!is_federation_sender_blocked("alice@example.org"));
    }

    #[test]
    fn blocks_reserved_rcpt_local_parts() {
        assert!(is_federation_rcpt_blocked("admin@example.org"));
        assert!(is_federation_rcpt_blocked("postmaster@example.org"));
        assert!(!is_federation_rcpt_blocked("alice@example.org"));
    }

    #[tokio::test]
    async fn unknown_user_not_allowed() {
        let pool = init_memory_db().await.unwrap();
        assert!(
            !inbound_local_recipient_allowed(&pool, "nobody@example.org")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn existing_user_allowed() {
        let pool = init_memory_db().await.unwrap();
        passwords::create_user(&pool, "u@example.org", "hash")
            .await
            .unwrap();
        assert!(
            inbound_local_recipient_allowed(&pool, "u@example.org")
                .await
                .unwrap()
        );
    }
}
