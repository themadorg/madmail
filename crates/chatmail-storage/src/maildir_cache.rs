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

//! Maildir listing cache keyed on directory mtimes (Dovecot `DIR_MTIME_CHANGED` fast path).

use std::path::Path;
use std::time::SystemTime;

use dashmap::DashMap;

use crate::maildir_message::StoredMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DirMtime {
    secs: u64,
    nanos: u32,
}

impl DirMtime {
    fn from_system_time(t: SystemTime) -> Self {
        let dur = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
        Self {
            secs: dur.as_secs(),
            nanos: dur.subsec_nanos(),
        }
    }
}

#[derive(Debug, Clone)]
struct CachedListing {
    new_mtime: Option<DirMtime>,
    cur_mtime: Option<DirMtime>,
    messages: Vec<StoredMessage>,
}

/// Per-mailbox listing cache invalidated when `new/` or `cur/` mtimes change.
#[derive(Debug, Default)]
pub struct MaildirListCache {
    entries: DashMap<(String, String), CachedListing>,
}

impl MaildirListCache {
    pub fn invalidate(&self, user: &str, mailbox: &str) {
        self.entries
            .remove(&(user.to_string(), mailbox.to_string()));
    }

    pub(crate) async fn dir_mtime(path: &Path) -> Option<DirMtime> {
        if !path.exists() {
            return None;
        }
        tokio::fs::metadata(path)
            .await
            .ok()
            .and_then(|m| m.modified().ok())
            .map(DirMtime::from_system_time)
    }

    pub async fn get_if_fresh(
        &self,
        user: &str,
        mailbox: &str,
        new_dir: &Path,
        cur_dir: &Path,
    ) -> Option<Vec<StoredMessage>> {
        let key = (user.to_string(), mailbox.to_string());
        let cached = self.entries.get(&key)?;
        let new_mtime = Self::dir_mtime(new_dir).await;
        let cur_mtime = Self::dir_mtime(cur_dir).await;
        if cached.new_mtime == new_mtime && cached.cur_mtime == cur_mtime {
            Some(cached.messages.clone())
        } else {
            None
        }
    }

    pub(crate) fn store(
        &self,
        user: &str,
        mailbox: &str,
        new_mtime: Option<DirMtime>,
        cur_mtime: Option<DirMtime>,
        messages: Vec<StoredMessage>,
    ) {
        self.entries.insert(
            (user.to_string(), mailbox.to_string()),
            CachedListing {
                new_mtime,
                cur_mtime,
                messages,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maildir_message::{MaildirFlags, StoredMessage};
    use std::time::SystemTime;

    fn sample_msg(id: &str) -> StoredMessage {
        StoredMessage {
            uid: 1,
            base_id: id.to_string(),
            filename: id.to_string(),
            size: 1,
            internal_date: SystemTime::now(),
            flags: MaildirFlags::default(),
        }
    }

    /// P11-UT04: listing cache hits when directory mtimes are unchanged.
    #[tokio::test]
    async fn p11_ut04_list_cache_hits_on_unchanged_mtime() {
        let tmp = tempfile::tempdir().unwrap();
        let new_dir = tmp.path().join("new");
        let cur_dir = tmp.path().join("cur");
        tokio::fs::create_dir_all(&new_dir).await.unwrap();
        tokio::fs::create_dir_all(&cur_dir).await.unwrap();

        let cache = MaildirListCache::default();
        let new_mtime = MaildirListCache::dir_mtime(&new_dir).await;
        let cur_mtime = MaildirListCache::dir_mtime(&cur_dir).await;
        let msgs = vec![sample_msg("a")];
        cache.store("u@test", "INBOX", new_mtime, cur_mtime, msgs.clone());

        let hit = cache
            .get_if_fresh("u@test", "INBOX", &new_dir, &cur_dir)
            .await
            .unwrap();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].base_id, "a");
    }

    /// P11-UT05: cache miss after a new message changes `new/` mtime.
    #[tokio::test]
    async fn p11_ut05_list_cache_misses_after_directory_change() {
        let tmp = tempfile::tempdir().unwrap();
        let new_dir = tmp.path().join("new");
        let cur_dir = tmp.path().join("cur");
        tokio::fs::create_dir_all(&new_dir).await.unwrap();
        tokio::fs::create_dir_all(&cur_dir).await.unwrap();

        let cache = MaildirListCache::default();
        let new_mtime = MaildirListCache::dir_mtime(&new_dir).await;
        let cur_mtime = MaildirListCache::dir_mtime(&cur_dir).await;
        cache.store(
            "u@test",
            "INBOX",
            new_mtime,
            cur_mtime,
            vec![sample_msg("old")],
        );

        tokio::fs::write(new_dir.join("msg"), b"x").await.unwrap();
        // Child file creation does not bump directory mtime on all filesystems (e.g. tmpfs);
        // production paths call `invalidate_mailbox_listing` on write. Simulate mtime change here.
        let bumped = filetime::FileTime::from_system_time(
            std::time::SystemTime::now() + std::time::Duration::from_secs(5),
        );
        filetime::set_file_mtime(&new_dir, bumped).unwrap();

        assert!(cache
            .get_if_fresh("u@test", "INBOX", &new_dir, &cur_dir)
            .await
            .is_none());
    }

    /// P11-UT06: explicit invalidation drops cached listing.
    #[tokio::test]
    async fn p11_ut06_invalidate_clears_entry() {
        let cache = MaildirListCache::default();
        cache.store("u@test", "INBOX", None, None, vec![sample_msg("x")]);
        cache.invalidate("u@test", "INBOX");
        assert!(cache
            .entries
            .get(&("u@test".into(), "INBOX".into()))
            .is_none());
    }
}
