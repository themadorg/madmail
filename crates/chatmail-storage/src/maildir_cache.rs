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

//! Maildir listing cache keyed on directory mtimes (Dovecot `DIR_MTIME_CHANGED` fast path)
//! plus a per-mailbox generation counter so concurrent delivery cannot re-poison the cache.
//!
//! ## Why a generation?
//!
//! Directory mtime alone is not enough:
//! 1. Some filesystems only update mtime at 1-second resolution — two deliveries in the same
//!    second can leave the cached listing missing the newer message while mtime still "matches".
//! 2. A concurrent `list_mailbox_messages` that started before a delivery can finish its
//!    `uidlist::sync` *after* `invalidate`, then `store` an incomplete listing under the new
//!    mtime. Subsequent hits serve that incomplete set until the next invalidate (#115).
//!
//! Every invalidate bumps the generation. `store` only inserts when the caller's generation
//! still matches, so a listing that raced with delivery never becomes the cached view.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
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
    /// Generation observed when this entry was written; must match current gen on hit.
    generation: u64,
    new_mtime: Option<DirMtime>,
    cur_mtime: Option<DirMtime>,
    messages: Vec<StoredMessage>,
}

/// Per-mailbox listing cache invalidated when `new/` or `cur/` change (or on explicit bump).
#[derive(Debug, Default)]
pub struct MaildirListCache {
    entries: DashMap<(String, String), CachedListing>,
    /// Monotonic per-mailbox epoch; bumped on every [`Self::invalidate`].
    generations: DashMap<(String, String), AtomicU64>,
}

impl MaildirListCache {
    fn key(user: &str, mailbox: &str) -> (String, String) {
        (user.to_string(), mailbox.to_string())
    }

    /// Current generation for `(user, mailbox)` (0 if never invalidated).
    pub fn generation(&self, user: &str, mailbox: &str) -> u64 {
        self.generations
            .get(&Self::key(user, mailbox))
            .map(|g| g.load(Ordering::Acquire))
            .unwrap_or(0)
    }

    pub fn invalidate(&self, user: &str, mailbox: &str) {
        let key = Self::key(user, mailbox);
        self.generations
            .entry(key.clone())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::AcqRel);
        self.entries.remove(&key);
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
        // Read mtimes before locking the shard: never hold a DashMap guard across `.await`.
        let new_mtime = Self::dir_mtime(new_dir).await;
        let cur_mtime = Self::dir_mtime(cur_dir).await;
        let gen = self.generation(user, mailbox);
        let key = Self::key(user, mailbox);
        let cached = self.entries.get(&key)?;
        if cached.generation == gen
            && cached.new_mtime == new_mtime
            && cached.cur_mtime == cur_mtime
        {
            Some(cached.messages.clone())
        } else {
            None
        }
    }

    /// Cache `messages` only if `expected_generation` is still the live generation for this
    /// mailbox. Callers snapshot the generation *before* a slow `uidlist::sync` so a concurrent
    /// delivery's invalidate discards the incomplete listing.
    pub(crate) fn store(
        &self,
        user: &str,
        mailbox: &str,
        expected_generation: u64,
        new_mtime: Option<DirMtime>,
        cur_mtime: Option<DirMtime>,
        messages: Vec<StoredMessage>,
    ) {
        if self.generation(user, mailbox) != expected_generation {
            return;
        }
        self.entries.insert(
            Self::key(user, mailbox),
            CachedListing {
                generation: expected_generation,
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
        let gen = cache.generation("u@test", "INBOX");
        let new_mtime = MaildirListCache::dir_mtime(&new_dir).await;
        let cur_mtime = MaildirListCache::dir_mtime(&cur_dir).await;
        let msgs = vec![sample_msg("a")];
        cache.store("u@test", "INBOX", gen, new_mtime, cur_mtime, msgs.clone());

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
        let gen = cache.generation("u@test", "INBOX");
        let new_mtime = MaildirListCache::dir_mtime(&new_dir).await;
        let cur_mtime = MaildirListCache::dir_mtime(&cur_dir).await;
        cache.store(
            "u@test",
            "INBOX",
            gen,
            new_mtime,
            cur_mtime,
            vec![sample_msg("old")],
        );

        tokio::fs::write(new_dir.join("msg"), b"x").await.unwrap();
        // Some filesystems keep the same second-granularity mtime; invalidate is the
        // production signal. Mimic a delivery that also bumps generation.
        cache.invalidate("u@test", "INBOX");

        assert!(cache
            .get_if_fresh("u@test", "INBOX", &new_dir, &cur_dir)
            .await
            .is_none());
    }

    /// Issue #115: a listing that races with delivery must not re-poison the cache after
    /// invalidate (stale incomplete listing under a matching mtime).
    #[tokio::test]
    async fn store_after_invalidate_is_discarded() {
        let tmp = tempfile::tempdir().unwrap();
        let new_dir = tmp.path().join("new");
        let cur_dir = tmp.path().join("cur");
        tokio::fs::create_dir_all(&new_dir).await.unwrap();
        tokio::fs::create_dir_all(&cur_dir).await.unwrap();

        let cache = MaildirListCache::default();
        let gen_before = cache.generation("u@test", "INBOX");
        let new_mtime = MaildirListCache::dir_mtime(&new_dir).await;
        let cur_mtime = MaildirListCache::dir_mtime(&cur_dir).await;

        // Delivery lands and invalidates while a concurrent list still holds gen_before.
        cache.invalidate("u@test", "INBOX");
        assert_ne!(cache.generation("u@test", "INBOX"), gen_before);

        // Incomplete listing tries to publish under the old generation — must be ignored.
        cache.store(
            "u@test",
            "INBOX",
            gen_before,
            new_mtime,
            cur_mtime,
            vec![sample_msg("stale-only")],
        );
        assert!(
            cache
                .entries
                .get(&("u@test".into(), "INBOX".into()))
                .is_none(),
            "store with stale generation must not insert"
        );

        // A correct post-delivery listing uses the new generation.
        let gen_after = cache.generation("u@test", "INBOX");
        cache.store(
            "u@test",
            "INBOX",
            gen_after,
            new_mtime,
            cur_mtime,
            vec![sample_msg("fresh")],
        );
        let hit = cache
            .get_if_fresh("u@test", "INBOX", &new_dir, &cur_dir)
            .await
            .expect("fresh listing should hit");
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].base_id, "fresh");
    }

    /// Same-second mtime: invalidate alone must force a miss even when dir mtime is unchanged.
    #[tokio::test]
    async fn invalidate_misses_even_when_mtime_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let new_dir = tmp.path().join("new");
        let cur_dir = tmp.path().join("cur");
        tokio::fs::create_dir_all(&new_dir).await.unwrap();
        tokio::fs::create_dir_all(&cur_dir).await.unwrap();

        let cache = MaildirListCache::default();
        let gen = cache.generation("u@test", "INBOX");
        let new_mtime = MaildirListCache::dir_mtime(&new_dir).await;
        let cur_mtime = MaildirListCache::dir_mtime(&cur_dir).await;
        cache.store(
            "u@test",
            "INBOX",
            gen,
            new_mtime,
            cur_mtime,
            vec![sample_msg("a")],
        );
        assert!(cache
            .get_if_fresh("u@test", "INBOX", &new_dir, &cur_dir)
            .await
            .is_some());

        cache.invalidate("u@test", "INBOX");
        assert!(cache
            .get_if_fresh("u@test", "INBOX", &new_dir, &cur_dir)
            .await
            .is_none());
    }

    /// `get_if_fresh` must not hold a DashMap shard guard across `.await`: a concurrent `store` on
    /// the same shard would block a single-worker runtime forever. Reproduce and bound with a
    /// watchdog — the buggy ordering hangs, the fixed ordering completes.
    #[test]
    fn get_if_fresh_does_not_deadlock_store() {
        let worker = std::thread::spawn(|| {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                let tmp = tempfile::tempdir().unwrap();
                let new_dir = tmp.path().join("new");
                let cur_dir = tmp.path().join("cur");
                tokio::fs::create_dir_all(&new_dir).await.unwrap();
                tokio::fs::create_dir_all(&cur_dir).await.unwrap();

                let cache = std::sync::Arc::new(MaildirListCache::default());
                let m = MaildirListCache::dir_mtime(&new_dir).await;
                let gen = cache.generation("u@test", "INBOX");
                cache.store("u@test", "INBOX", gen, m, m, vec![sample_msg("a")]);

                let reader = {
                    let cache = cache.clone();
                    let new_dir = new_dir.clone();
                    let cur_dir = cur_dir.clone();
                    tokio::spawn(async move {
                        cache
                            .get_if_fresh("u@test", "INBOX", &new_dir, &cur_dir)
                            .await
                    })
                };
                let writer = {
                    let cache = cache.clone();
                    tokio::spawn(async move {
                        for i in 0..50 {
                            let g = cache.generation("u@test", "INBOX");
                            cache.store("u@test", "INBOX", g, None, None, vec![sample_msg("b")]);
                            cache.invalidate("u@test", "INBOX");
                            if i % 7 == 0 {
                                tokio::task::yield_now().await;
                            }
                        }
                    })
                };
                let _ = reader.await;
                let _ = writer.await;
            });
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !worker.is_finished() {
            if std::time::Instant::now() > deadline {
                panic!("get_if_fresh deadlocked with a concurrent store");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        worker.join().unwrap();
    }

    /// P11-UT06: explicit invalidation drops cached listing.
    #[tokio::test]
    async fn p11_ut06_invalidate_clears_entry() {
        let cache = MaildirListCache::default();
        let gen = cache.generation("u@test", "INBOX");
        cache.store("u@test", "INBOX", gen, None, None, vec![sample_msg("x")]);
        cache.invalidate("u@test", "INBOX");
        assert!(cache
            .entries
            .get(&("u@test".into(), "INBOX".into()))
            .is_none());
    }
}
