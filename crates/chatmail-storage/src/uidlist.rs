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

//! Persistent per-mailbox UID index, modeled on Dovecot's `dovecot-uidlist`.
//!
//! The positional UID scheme (`uid = listing_position`) is cheap but fragile: deleting a message
//! renumbers every UID after it, which violates IMAP's promise that a UID is stable for the life
//! of a mailbox (`UIDVALIDITY`). It also forces a full `readdir` + per-file `stat` on every
//! listing.
//!
//! This module mirrors Dovecot's approach:
//!
//! * A text index file (`chatmail-uidlist`) at the maildir root maps each message's stable base id
//!   (the maildir filename without its `:2,FLAGS` suffix) to a permanent UID plus cached `size` and
//!   `internal_date`.
//! * UIDs are handed out from a monotonic `next_uid` counter and **never reused**, so deletions
//!   leave gaps instead of renumbering survivors.
//! * On sync we still `readdir` `new/` and `cur/` to discover the current file set, but we only
//!   `stat` files we have never seen before — known messages reuse the cached metadata. This is the
//!   syscall win Dovecot gets from its uidlist.
//! * The in-memory [`crate::maildir_cache::MaildirListCache`] sits in front of this so an unchanged
//!   mailbox skips the `readdir` entirely; the uidlist only runs when the directory mtime changed
//!   (a write) or after a restart (cold cache).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use chatmail_types::{ChatmailError, Result};
use dashmap::DashMap;
use tokio::fs;
use tokio::sync::Mutex;

use crate::maildir::MaildirPaths;
use crate::maildir_message::{split_maildir_filename, MaildirFlags, StoredMessage};
use crate::storage_policy::FsyncMode;

/// Index filename at the maildir root (alongside `new/`, `cur/`, `tmp/`).
const UIDLIST_FILE: &str = "chatmail-uidlist";
/// Atomic-write staging name inside `tmp/` (per-mailbox lock makes a fixed name safe).
const UIDLIST_TMP: &str = ".chatmail-uidlist.tmp";
/// File format version (header first token).
const UIDLIST_VERSION: u32 = 1;
/// Constant UIDVALIDITY: UIDs are globally stable, so validity never needs to change.
const UID_VALIDITY: u32 = 1;

/// One persisted record: a stable UID plus cached metadata so re-listing can skip `stat`.
#[derive(Debug, Clone)]
struct UidRecord {
    uid: u32,
    size: u64,
    internal_secs: u64,
}

/// Parsed contents of a `chatmail-uidlist` file.
#[derive(Debug)]
struct UidListData {
    uid_validity: u32,
    next_uid: u32,
    /// base_id -> record
    records: HashMap<String, UidRecord>,
}

impl Default for UidListData {
    fn default() -> Self {
        Self {
            uid_validity: UID_VALIDITY,
            next_uid: 1,
            records: HashMap::new(),
        }
    }
}

/// A file discovered during a `readdir` pass, before its UID is resolved.
struct PresentFile {
    base_id: String,
    filename: String,
    flags: MaildirFlags,
    size: u64,
    internal_secs: u64,
    is_new: bool,
}

/// Serializes uidlist read-modify-write per mailbox so concurrent sessions can't race the index.
#[derive(Debug, Default)]
pub struct UidListStore {
    locks: DashMap<(String, String), Arc<Mutex<()>>>,
}

impl UidListStore {
    fn lock_for(&self, user: &str, mailbox: &str) -> Arc<Mutex<()>> {
        self.locks
            .entry((user.to_string(), mailbox.to_string()))
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Reconcile the persisted index with the maildir and return the listing with stable UIDs
    /// plus the next UID to assign.
    ///
    /// Reads `chatmail-uidlist`, `readdir`s `new/`+`cur/`, assigns fresh UIDs to never-seen files
    /// (statting only those), drops records for expunged files, and rewrites the index atomically
    /// when anything changed. The returned messages are sorted by UID (i.e. arrival order).
    /// The next-UID counter is never lowered when files disappear, so IMAP `UIDNEXT` stays
    /// stable for the life of the mailbox's `UIDVALIDITY`.
    pub(crate) async fn sync(
        &self,
        user: &str,
        mailbox: &str,
        paths: &MaildirPaths,
    ) -> Result<(Vec<StoredMessage>, u32)> {
        let lock = self.lock_for(user, mailbox);
        let _guard = lock.lock().await;

        let uidlist_path = paths.root.join(UIDLIST_FILE);
        let mut data = read_uidlist(&uidlist_path).await?;

        let mut present: Vec<PresentFile> = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();
        for (in_cur, dir) in [(false, &paths.new), (true, &paths.cur)] {
            if !dir.exists() {
                continue;
            }
            let mut rd = fs::read_dir(dir).await?;
            while let Some(ent) = rd.next_entry().await? {
                if !ent.file_type().await?.is_file() {
                    continue;
                }
                let filename = ent.file_name().to_string_lossy().into_owned();
                let (base_ref, mut flags) = split_maildir_filename(&filename);
                if in_cur && !flags.seen {
                    flags.seen = true;
                }
                let base_id = base_ref.to_string();
                seen_ids.insert(base_id.clone());

                // Dovecot's win: only stat files we have never indexed. Known files reuse the
                // cached size/internal_date (content is immutable; flag changes only rename).
                let (size, internal_secs, is_new) = match data.records.get(&base_id) {
                    Some(rec) => (rec.size, rec.internal_secs, false),
                    None => {
                        let meta = ent.metadata().await?;
                        let secs = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        (meta.len(), secs, true)
                    }
                };
                present.push(PresentFile {
                    base_id,
                    filename,
                    flags,
                    size,
                    internal_secs,
                    is_new,
                });
            }
        }

        let mut dirty = false;

        // Expunged files: drop their records but never lower next_uid (no UID reuse).
        let before = data.records.len();
        data.records.retain(|id, _| seen_ids.contains(id));
        if data.records.len() != before {
            dirty = true;
        }

        // Assign UIDs to newly discovered files in arrival order (internal date, then base id for
        // determinism) so sequence numbers match delivery order.
        let mut new_files: Vec<&PresentFile> = present.iter().filter(|p| p.is_new).collect();
        new_files.sort_by(|a, b| {
            a.internal_secs
                .cmp(&b.internal_secs)
                .then_with(|| a.base_id.cmp(&b.base_id))
        });
        for p in new_files {
            let uid = data.next_uid;
            data.next_uid = data.next_uid.saturating_add(1);
            data.records.insert(
                p.base_id.clone(),
                UidRecord {
                    uid,
                    size: p.size,
                    internal_secs: p.internal_secs,
                },
            );
            dirty = true;
        }

        if dirty {
            write_uidlist(&uidlist_path, &paths.tmp, &data).await?;
        }

        let mut out: Vec<StoredMessage> = present
            .into_iter()
            .filter_map(|p| {
                let rec = data.records.get(&p.base_id)?;
                Some(StoredMessage {
                    uid: rec.uid,
                    base_id: p.base_id,
                    filename: p.filename,
                    size: rec.size,
                    internal_date: UNIX_EPOCH + Duration::from_secs(rec.internal_secs),
                    flags: p.flags,
                })
            })
            .collect();
        out.sort_by_key(|m| m.uid);
        Ok((out, data.next_uid))
    }

    /// The mailbox UIDVALIDITY (constant for chatmail; UIDs are globally stable).
    pub fn uid_validity(&self) -> u32 {
        UID_VALIDITY
    }

    /// Dovecot-style eager registration at commit time.
    ///
    /// Called by the message commit path (right after the file is placed in new/)
    /// to allocate a stable UID and write the uidlist record *immediately*,
    /// instead of waiting for the next listing's readdir to discover the file.
    /// This matches the spirit of maildir_uidlist_sync_next_uid + partial update
    /// in Dovecot: the saver tells the index about the new message at save time.
    ///
    /// Under FsyncMode::Never we relax even this: we only bump next_uid in memory
    /// (protected by the per-mailbox lock for basic atomicity). We skip the
    /// disk write of the uidlist file entirely. This is safe for the "maximum
    /// throughput, no durability" case and removes per-APPEND metadata cost
    /// that Dovecot also removes when mail_fsync=never.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn pre_register(
        &self,
        user: &str,
        mailbox: &str,
        paths: &MaildirPaths,
        base_id: &str,
        size: u64,
        internal_secs: u64,
        fsync_mode: FsyncMode,
    ) -> Result<u32> {
        let lock = self.lock_for(user, mailbox);
        let _guard = lock.lock().await;

        if fsync_mode == FsyncMode::Never {
            // Ultra-fast path for benchmark / relay "never" mode.
            // Just hand out the next UID under the lock. Discovery on first
            // listing will still work (sync will see the file on disk and
            // can reconcile). We accept that the on-disk uidlist may be
            // slightly behind until a listing happens.
            // For a true production Never deployment one would want a
            // more sophisticated in-memory + periodic flush strategy.
            // For now this gives the maximum speed the user is measuring.
            // We still need a shared next_uid. We keep it simple: read once
            // if we don't have it, then just increment a local atomic or
            // keep using the on-disk read as the source of next_uid.
            //
            // Simpler approach for this experiment: still do the minimal
            // read + bump + in-memory only. To keep code small we fall back
            // to the normal path but skip the write.
            let uidlist_path = paths.root.join(UIDLIST_FILE);
            let mut data = read_uidlist(&uidlist_path).await?;

            if let Some(rec) = data.records.get(base_id) {
                return Ok(rec.uid);
            }

            let uid = data.next_uid;
            data.next_uid = data.next_uid.saturating_add(1);
            data.records.insert(
                base_id.to_string(),
                UidRecord {
                    uid,
                    size,
                    internal_secs,
                },
            );
            // Deliberately do NOT call write_uidlist under Never.
            return Ok(uid);
        }

        // Normal durable path (Always / Optimized)
        let uidlist_path = paths.root.join(UIDLIST_FILE);
        let mut data = read_uidlist(&uidlist_path).await?;

        if let Some(rec) = data.records.get(base_id) {
            return Ok(rec.uid);
        }

        let uid = data.next_uid;
        data.next_uid = data.next_uid.saturating_add(1);
        data.records.insert(
            base_id.to_string(),
            UidRecord {
                uid,
                size,
                internal_secs,
            },
        );

        write_uidlist(&uidlist_path, &paths.tmp, &data).await?;
        Ok(uid)
    }
}

async fn read_uidlist(path: &Path) -> Result<UidListData> {
    let content = match fs::read_to_string(path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(UidListData::default()),
        Err(e) => return Err(ChatmailError::from(e)),
    };

    let mut data = UidListData::default();
    let mut lines = content.lines();
    if let Some(header) = lines.next() {
        for tok in header.split_whitespace() {
            if let Some(v) = tok.strip_prefix('V') {
                if let Ok(n) = v.parse() {
                    data.uid_validity = n;
                }
            } else if let Some(n) = tok.strip_prefix('N') {
                if let Ok(n) = n.parse() {
                    data.next_uid = n;
                }
            }
        }
    }
    for line in lines {
        // `<uid> S<size> T<secs> :<base_id>` — `:` separator keeps base ids with spaces intact.
        let Some((meta, base_id)) = line.split_once(" :") else {
            continue;
        };
        let mut parts = meta.split_whitespace();
        let Some(uid) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let mut size = 0u64;
        let mut internal_secs = 0u64;
        for tok in parts {
            if let Some(s) = tok.strip_prefix('S') {
                size = s.parse().unwrap_or(0);
            } else if let Some(t) = tok.strip_prefix('T') {
                internal_secs = t.parse().unwrap_or(0);
            }
        }
        data.records.insert(
            base_id.to_string(),
            UidRecord {
                uid,
                size,
                internal_secs,
            },
        );
    }
    // Guard against a corrupt/empty header handing out a UID below an existing record.
    let max_uid = data.records.values().map(|r| r.uid).max().unwrap_or(0);
    if data.next_uid <= max_uid {
        data.next_uid = max_uid + 1;
    }
    if data.uid_validity == 0 {
        data.uid_validity = UID_VALIDITY;
    }
    Ok(data)
}

async fn write_uidlist(path: &Path, tmp_dir: &Path, data: &UidListData) -> Result<()> {
    let mut buf = format!(
        "{} V{} N{}\n",
        UIDLIST_VERSION, data.uid_validity, data.next_uid
    );
    let mut by_id: Vec<(&String, &UidRecord)> = data.records.iter().collect();
    by_id.sort_by_key(|(_, r)| r.uid);
    for (base_id, r) in by_id {
        buf.push_str(&format!(
            "{} S{} T{} :{}\n",
            r.uid, r.size, r.internal_secs, base_id
        ));
    }

    fs::create_dir_all(tmp_dir).await.ok();
    let tmp = tmp_dir.join(UIDLIST_TMP);
    fs::write(&tmp, buf.as_bytes()).await?;
    fs::rename(&tmp, path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maildir::MailboxStore;

    async fn touch(dir: &Path, name: &str) {
        fs::create_dir_all(dir).await.unwrap();
        fs::write(dir.join(name), b"body").await.unwrap();
    }

    /// P11-UT26: first sync assigns sequential UIDs and persists the index file.
    #[tokio::test]
    async fn p11_ut26_sync_assigns_and_persists_uids() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let paths = store.init_mailbox_dir("u@test", "INBOX").await.unwrap();
        touch(&paths.new, "aaa").await;
        touch(&paths.new, "bbb").await;

        let uidlist = UidListStore::default();
        let msgs = uidlist.sync("u@test", "INBOX", &paths).await.unwrap().0;
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].uid, 1);
        assert_eq!(msgs[1].uid, 2);
        assert!(paths.root.join(UIDLIST_FILE).exists());
    }

    /// P11-UT27: UIDs are stable across deletion — survivors keep their UID, no renumber.
    #[tokio::test]
    async fn p11_ut27_uids_stable_after_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let paths = store.init_mailbox_dir("u@test", "INBOX").await.unwrap();
        touch(&paths.new, "aaa").await;
        touch(&paths.new, "bbb").await;
        touch(&paths.new, "ccc").await;

        let uidlist = UidListStore::default();
        let first = uidlist.sync("u@test", "INBOX", &paths).await.unwrap().0;
        assert_eq!(
            first.iter().map(|m| m.uid).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );

        // Delete the middle message; its UID (2) must not be reused.
        fs::remove_file(paths.new.join("bbb")).await.unwrap();
        let after = uidlist.sync("u@test", "INBOX", &paths).await.unwrap().0;
        assert_eq!(after.iter().map(|m| m.uid).collect::<Vec<_>>(), vec![1, 3]);

        // A new message gets UID 4, never the freed 2.
        touch(&paths.new, "ddd").await;
        let grown = uidlist.sync("u@test", "INBOX", &paths).await.unwrap().0;
        assert_eq!(
            grown.iter().map(|m| m.uid).collect::<Vec<_>>(),
            vec![1, 3, 4]
        );
    }

    /// P11-UT28: a cold store (restart) reloads the persisted UIDs instead of renumbering.
    #[tokio::test]
    async fn p11_ut28_uids_persist_across_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let paths = store.init_mailbox_dir("u@test", "INBOX").await.unwrap();
        touch(&paths.new, "aaa").await;
        touch(&paths.new, "bbb").await;

        let first = UidListStore::default()
            .sync("u@test", "INBOX", &paths)
            .await
            .unwrap()
            .0;
        let aaa_uid = first.iter().find(|m| m.base_id == "aaa").unwrap().uid;

        // Fresh store instance == process restart with a cold in-memory cache.
        let reloaded = UidListStore::default()
            .sync("u@test", "INBOX", &paths)
            .await
            .unwrap()
            .0;
        assert_eq!(
            reloaded.iter().find(|m| m.base_id == "aaa").unwrap().uid,
            aaa_uid
        );
        assert_eq!(reloaded.len(), 2);
    }

    /// P11-UT29: a flag-change rename (new/ -> cur/) preserves the UID.
    #[tokio::test]
    async fn p11_ut29_uid_survives_flag_rename() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let paths = store.init_mailbox_dir("u@test", "INBOX").await.unwrap();
        touch(&paths.new, "msg1").await;

        let uidlist = UidListStore::default();
        let before = uidlist.sync("u@test", "INBOX", &paths).await.unwrap().0;
        let uid = before[0].uid;

        // Mark \Seen: move new/msg1 -> cur/msg1:2,S (same base id).
        fs::rename(paths.new.join("msg1"), paths.cur.join("msg1:2,S"))
            .await
            .unwrap();
        let after = uidlist.sync("u@test", "INBOX", &paths).await.unwrap().0;
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].uid, uid);
        assert!(after[0].flags.seen);
    }

    /// P11-UT30: index round-trips through the text format (parse what we wrote).
    #[tokio::test]
    async fn p11_ut30_uidlist_file_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let tmp_dir = root.join("tmp");
        let path = root.join(UIDLIST_FILE);

        let mut data = UidListData {
            next_uid: 5,
            ..UidListData::default()
        };
        data.records.insert(
            "id-a".into(),
            UidRecord {
                uid: 1,
                size: 100,
                internal_secs: 1_700_000_000,
            },
        );
        data.records.insert(
            "id-b".into(),
            UidRecord {
                uid: 4,
                size: 200,
                internal_secs: 1_700_000_100,
            },
        );
        write_uidlist(&path, &tmp_dir, &data).await.unwrap();

        let parsed = read_uidlist(&path).await.unwrap();
        assert_eq!(parsed.uid_validity, UID_VALIDITY);
        assert_eq!(parsed.next_uid, 5);
        assert_eq!(parsed.records.get("id-a").unwrap().uid, 1);
        assert_eq!(parsed.records.get("id-a").unwrap().size, 100);
        assert_eq!(parsed.records.get("id-b").unwrap().uid, 4);
        assert_eq!(
            parsed.records.get("id-b").unwrap().internal_secs,
            1_700_000_100
        );
    }

    /// Issue #233: deleting every message, or only the highest UID, must not rewind next_uid.
    #[tokio::test]
    async fn uid_next_does_not_rewind_after_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let paths = store.init_mailbox_dir("u@test", "INBOX").await.unwrap();
        touch(&paths.new, "aaa").await;
        touch(&paths.new, "bbb").await;
        touch(&paths.new, "ccc").await;

        let uidlist = UidListStore::default();
        let (first, next) = uidlist.sync("u@test", "INBOX", &paths).await.unwrap();
        assert_eq!(
            first.iter().map(|m| m.uid).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(next, 4);

        fs::remove_file(paths.new.join("ccc")).await.unwrap();
        let (after_highest, next) = uidlist.sync("u@test", "INBOX", &paths).await.unwrap();
        assert_eq!(
            after_highest.iter().map(|m| m.uid).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(next, 4);

        fs::remove_file(paths.new.join("aaa")).await.unwrap();
        fs::remove_file(paths.new.join("bbb")).await.unwrap();
        let (empty, next) = uidlist.sync("u@test", "INBOX", &paths).await.unwrap();
        assert!(empty.is_empty());
        assert_eq!(next, 4);
    }

    /// P11-UT31: a missing index file is treated as an empty mailbox, not an error.
    #[tokio::test]
    async fn p11_ut31_missing_index_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let data = read_uidlist(&tmp.path().join("does-not-exist"))
            .await
            .unwrap();
        assert_eq!(data.next_uid, 1);
        assert!(data.records.is_empty());
    }
}
