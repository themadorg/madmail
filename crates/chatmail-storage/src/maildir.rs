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

use std::path::{Path, PathBuf};

use chatmail_types::{ChatmailError, Result};

/// Maildir layout under `{state_dir}/mail/{user}/Maildir/`.
#[derive(Debug, Clone)]
pub struct MaildirPaths {
    pub root: PathBuf,
    pub cur: PathBuf,
    pub new: PathBuf,
    pub tmp: PathBuf,
}

#[derive(Debug, Clone)]
pub struct MailboxStore {
    state_dir: PathBuf,
}

impl MailboxStore {
    pub fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            state_dir: state_dir.into(),
        }
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// INBOX maildir (`mail/{user}/Maildir/`).
    pub fn maildir_for_user(&self, user: &str) -> MaildirPaths {
        self.maildir_for_mailbox(user, "INBOX")
    }

    /// Per-mailbox maildir (INBOX or `folders/{name}/Maildir/`).
    pub fn maildir_for_mailbox(&self, user: &str, mailbox: &str) -> MaildirPaths {
        let root = if mailbox.eq_ignore_ascii_case("INBOX") {
            self.state_dir
                .join("mail")
                .join(sanitize_user(user))
                .join("Maildir")
        } else {
            self.state_dir
                .join("mail")
                .join(sanitize_user(user))
                .join("folders")
                .join(sanitize_mailbox(mailbox))
                .join("Maildir")
        };
        MaildirPaths {
            cur: root.join("cur"),
            new: root.join("new"),
            tmp: root.join("tmp"),
            root,
        }
    }

    /// Create Maildir tree for a user (`cur`, `new`, `tmp`).
    pub async fn init_user_dir(&self, user: &str) -> Result<MaildirPaths> {
        self.init_mailbox_dir(user, "INBOX").await
    }

    /// Create Maildir tree for a mailbox if missing.
    pub async fn init_mailbox_dir(&self, user: &str, mailbox: &str) -> Result<MaildirPaths> {
        let paths = self.maildir_for_mailbox(user, mailbox);
        for dir in [&paths.root, &paths.cur, &paths.new, &paths.tmp] {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(ChatmailError::from)?;
        }
        Ok(paths)
    }

    /// Sum file sizes under `cur/` and `new/` for quota hydration.
    pub async fn maildir_used_bytes(&self, user: &str) -> Result<u64> {
        let paths = self.maildir_for_user(user);
        let mut total = 0u64;
        for dir in [&paths.cur, &paths.new] {
            if !dir.exists() {
                continue;
            }
            total += dir_size(dir).await?;
        }
        Ok(total)
    }
}

fn sanitize_user(user: &str) -> String {
    user.replace(['/', '\\'], "_")
}

fn sanitize_mailbox(mailbox: &str) -> String {
    mailbox.replace(['/', '\\'], "_")
}

/// True if the mailbox has been initialized (Maildir root exists).
pub async fn mailbox_exists(store: &MailboxStore, user: &str, mailbox: &str) -> bool {
    store.maildir_for_mailbox(user, mailbox).root.exists()
}

/// fsync a directory so a preceding `rename`/`hard_link` into it survives a crash.
///
/// `File::sync_data` makes the *content* durable, but the directory entry that points at the
/// renamed/linked file may not be flushed until the kernel decides to (ext4 `data=ordered`, xfs,
/// btrfs, NFS …). Combined with notifying clients right after delivery, this is the classic
/// maildir "the client was told about a message whose file vanished on reboot" hole. Calling this
/// after the rename/link closes the window. On non-Unix targets it is a no-op (directory fds are
/// not fsync-able there).
#[cfg(unix)]
pub(crate) async fn fsync_dir(dir: &Path) -> Result<()> {
    let dir = tokio::fs::File::open(dir)
        .await
        .map_err(ChatmailError::from)?;
    dir.sync_all().await.map_err(ChatmailError::from)
}

#[cfg(not(unix))]
pub(crate) async fn fsync_dir(_dir: &Path) -> Result<()> {
    Ok(())
}

async fn dir_size(dir: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut read_dir = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let meta = entry.metadata().await?;
        if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P2-UT01: Maildir tree is created.
    #[tokio::test]
    async fn p2_ut01_test_maildir_init() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let paths = store.init_user_dir("alice@example.org").await.unwrap();

        assert!(paths.cur.is_dir());
        assert!(paths.new.is_dir());
        assert!(paths.tmp.is_dir());
        assert!(paths.root.ends_with("mail/alice@example.org/Maildir"));
    }
}
