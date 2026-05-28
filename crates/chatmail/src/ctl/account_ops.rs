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

//! Shared account CRUD for operator CLI (mirrors admin `provision` / `delete_account_full`).

use chatmail_db::{passwords, registration_tokens, DbPool};
use chatmail_storage::MailboxStore;
use chatmail_types::Result;

pub async fn delete_account_full(
    pool: &DbPool,
    mailbox: &MailboxStore,
    username: &str,
    reason: &str,
) -> Result<()> {
    let maildir = mailbox.maildir_for_user(username);
    if maildir.root.exists() {
        tokio::fs::remove_dir_all(&maildir.root).await?;
    }
    passwords::delete_user_full(pool, username, reason).await?;
    Ok(())
}

pub async fn provision_account(
    pool: &DbPool,
    mailbox: &MailboxStore,
    username: &str,
    stored_hash: &str,
) -> Result<()> {
    passwords::create_user(pool, username, stored_hash).await?;
    mailbox.init_user_dir(username).await?;
    registration_tokens::ensure_new_account_quota(pool, username).await?;
    Ok(())
}

pub fn is_internal_settings_key(username: &str) -> bool {
    username.starts_with("__") && username.ends_with("__")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_auth::hash_password;
    use chatmail_db::{blocklist, init_memory_db, passwords};

    #[test]
    fn internal_settings_keys_detected() {
        assert!(is_internal_settings_key("__REGISTRATION_OPEN__"));
        assert!(!is_internal_settings_key("user@example.org"));
    }

    #[tokio::test]
    async fn delete_account_full_removes_mail_and_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_memory_db().await.unwrap();
        let mailbox = MailboxStore::new(dir.path());
        let user = "gone@example.org";
        let hash = hash_password("secret123456").unwrap();
        provision_account(&pool, &mailbox, user, &hash)
            .await
            .unwrap();
        assert!(mailbox.maildir_for_user(user).root.exists());

        delete_account_full(&pool, &mailbox, user, "test delete").await.unwrap();
        assert!(!passwords::user_exists(&pool, user).await.unwrap());
        assert!(!mailbox.maildir_for_user(user).root.exists());
        assert!(blocklist::is_blocked(&pool, user).await.unwrap());
    }
}
