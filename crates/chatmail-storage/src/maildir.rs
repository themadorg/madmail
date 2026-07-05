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
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use chatmail_types::{ChatmailError, Result};

use crate::cas::ContentStore;
use crate::fsync_batch::FsyncCoordinator;
use crate::maildir_cache::MaildirListCache;
use crate::maildir_message::{MaildirFlags, StoredMessage};
use crate::storage_policy::StoragePolicy;
use crate::uidlist::UidListStore;

/// Maildir layout under `{state_dir}/mail/{user}/Maildir/`.
#[derive(Debug, Clone)]
pub struct MaildirPaths {
    pub root: PathBuf,
    pub cur: PathBuf,
    pub new: PathBuf,
    pub tmp: PathBuf,
}

#[derive(Debug)]
struct MailboxStoreInner {
    policy: StoragePolicy,
    list_cache: MaildirListCache,
    uidlist: UidListStore,
    fsync: FsyncCoordinator,
    content_store: ContentStore,
}

/// On-disk maildir access with listing cache, fsync policy, and optional CAS dedup.
#[derive(Debug, Clone)]
pub struct MailboxStore {
    state_dir: PathBuf,
    inner: Arc<MailboxStoreInner>,
}

impl MailboxStore {
    pub fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self::with_policy(state_dir, StoragePolicy::default())
    }

    pub fn with_policy(state_dir: impl Into<PathBuf>, policy: StoragePolicy) -> Self {
        let state_dir = state_dir.into();
        let fsync_mode = policy.fsync_mode;
        Self {
            state_dir: state_dir.clone(),
            inner: Arc::new(MailboxStoreInner {
                policy,
                list_cache: MaildirListCache::default(),
                uidlist: UidListStore::default(),
                fsync: FsyncCoordinator::new(fsync_mode),
                content_store: ContentStore::new(&state_dir),
            }),
        }
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub fn policy(&self) -> &StoragePolicy {
        &self.inner.policy
    }

    pub(crate) fn list_cache(&self) -> &MaildirListCache {
        &self.inner.list_cache
    }

    pub(crate) fn uidlist(&self) -> &UidListStore {
        &self.inner.uidlist
    }

    /// Mailbox UIDVALIDITY exposed to the IMAP layer (constant; UIDs are globally stable).
    pub fn uid_validity(&self) -> u32 {
        self.inner.uidlist.uid_validity()
    }

    pub(crate) fn fsync(&self) -> &FsyncCoordinator {
        &self.inner.fsync
    }

    pub(crate) fn content_store(&self) -> &ContentStore {
        &self.inner.content_store
    }

    pub fn invalidate_mailbox_listing(&self, user: &str, mailbox: &str) {
        self.inner.list_cache.invalidate(user, mailbox);
    }

    /// Register a newly delivered message in the uidlist and extend the listing cache.
    ///
    /// Call after the maildir file is visible in `new/`. Skips `invalidate_mailbox_listing` so
    /// concurrent IMAP FETCH does not force a full `uidlist.sync()` readdir (issue #68).
    pub async fn register_delivered_message(
        &self,
        user: &str,
        mailbox: &str,
        paths: &MaildirPaths,
        msg_id: &str,
        size: u64,
        internal_secs: u64,
    ) -> Result<StoredMessage> {
        let uid = self
            .uidlist()
            .pre_register(
                user,
                mailbox,
                paths,
                msg_id,
                size,
                internal_secs,
                self.policy().fsync_mode,
            )
            .await?;
        let message = StoredMessage {
            uid,
            base_id: msg_id.to_string(),
            filename: msg_id.to_string(),
            size,
            internal_date: UNIX_EPOCH + Duration::from_secs(internal_secs),
            flags: MaildirFlags::default(),
        };
        self.list_cache()
            .append_message(user, mailbox, &paths.new, &paths.cur, message.clone())
            .await;
        Ok(message)
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
/// Prefer [`FsyncCoordinator::commit_directory`] for policy-aware durability.
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
    use crate::storage_policy::FsyncMode;

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

    /// P11-UT10: custom storage policy is retained on the store handle.
    #[test]
    fn p11_ut10_mailbox_store_with_policy() {
        let policy = StoragePolicy {
            fsync_mode: FsyncMode::Never,
            cas_enabled: false,
            ..StoragePolicy::default()
        };
        let store = MailboxStore::with_policy("/tmp/x", policy.clone());
        assert_eq!(store.policy().fsync_mode, FsyncMode::Never);
        assert!(!store.policy().cas_enabled);
    }
}
