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

use std::path::PathBuf;
use std::time::SystemTime;

use chatmail_types::{ChatmailError, Result};
use tokio::fs;

use crate::maildir::MailboxStore;

/// Parsed maildir filename flags (`:2,XY` suffix).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaildirFlags {
    pub seen: bool,
    pub deleted: bool,
}

impl MaildirFlags {
    pub fn from_maildir_suffix(suffix: &str) -> Self {
        Self {
            seen: suffix.contains('S'),
            deleted: suffix.contains('T'),
        }
    }

    pub fn to_maildir_suffix(&self) -> String {
        let mut s = String::new();
        if self.seen {
            s.push('S');
        }
        if self.deleted {
            s.push('T');
        }
        s
    }

    pub fn imap_flags(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.seen {
            out.push("\\Seen");
        }
        if self.deleted {
            out.push("\\Deleted");
        }
        out
    }
}

/// One message on disk (maildir `new/` or `cur/`).
#[derive(Debug, Clone)]
pub struct StoredMessage {
    /// Stable IMAP UID assigned by the persistent uidlist (never reused, monotonic per mailbox).
    pub uid: u32,
    pub base_id: String,
    pub filename: String,
    pub size: u64,
    pub internal_date: SystemTime,
    pub flags: MaildirFlags,
}

pub fn split_maildir_filename(name: &str) -> (&str, MaildirFlags) {
    if let Some(pos) = name.find(":2,") {
        let base = &name[..pos];
        let flags = MaildirFlags::from_maildir_suffix(&name[pos + 3..]);
        (base, flags)
    } else {
        (name, MaildirFlags::default())
    }
}

pub fn maildir_filename(base_id: &str, flags: &MaildirFlags) -> String {
    let suffix = flags.to_maildir_suffix();
    if suffix.is_empty() {
        base_id.to_string()
    } else {
        format!("{base_id}:2,{suffix}")
    }
}

/// List messages in a mailbox, ordered by stable UID (oldest first).
///
/// Backed by the persistent [`crate::uidlist`] index: an unchanged mailbox is served from the
/// in-memory [`crate::maildir_cache`] fast path (no `readdir`), and on a real change the uidlist
/// reconciles the directory while only statting never-before-seen files.
pub async fn list_mailbox_messages(
    store: &MailboxStore,
    user: &str,
    mailbox: &str,
) -> Result<Vec<StoredMessage>> {
    let paths = store.maildir_for_mailbox(user, mailbox);
    if let Some(cached) = store
        .list_cache()
        .get_if_fresh(user, mailbox, &paths.new, &paths.cur)
        .await
    {
        return Ok(cached);
    }

    // Snapshot generation before the slow sync so a concurrent delivery's invalidate
    // discards any incomplete listing we might try to publish (#115).
    let gen = store.list_cache().generation(user, mailbox);
    let new_mtime = crate::maildir_cache::MaildirListCache::dir_mtime(&paths.new).await;
    let cur_mtime = crate::maildir_cache::MaildirListCache::dir_mtime(&paths.cur).await;
    let mut messages = store.uidlist().sync(user, mailbox, &paths).await?;

    // If the directory changed while we were scanning (or generation advanced), re-sync once
    // so we do not return a half-complete view to IMAP IDLE / FETCH.
    let new_mtime_after = crate::maildir_cache::MaildirListCache::dir_mtime(&paths.new).await;
    let cur_mtime_after = crate::maildir_cache::MaildirListCache::dir_mtime(&paths.cur).await;
    let gen_after = store.list_cache().generation(user, mailbox);
    if gen_after != gen || new_mtime_after != new_mtime || cur_mtime_after != cur_mtime {
        messages = store.uidlist().sync(user, mailbox, &paths).await?;
        let gen_final = store.list_cache().generation(user, mailbox);
        let new_final = crate::maildir_cache::MaildirListCache::dir_mtime(&paths.new).await;
        let cur_final = crate::maildir_cache::MaildirListCache::dir_mtime(&paths.cur).await;
        store.list_cache().store(
            user,
            mailbox,
            gen_final,
            new_final,
            cur_final,
            messages.clone(),
        );
        return Ok(messages);
    }

    store
        .list_cache()
        .store(user, mailbox, gen, new_mtime, cur_mtime, messages.clone());
    Ok(messages)
}

async fn locate_message(
    store: &MailboxStore,
    user: &str,
    mailbox: &str,
    base_id: &str,
) -> Result<(PathBuf, MaildirFlags, bool)> {
    let paths = store.maildir_for_mailbox(user, mailbox);
    for (in_cur, dir) in [(false, &paths.new), (true, &paths.cur)] {
        if !dir.exists() {
            continue;
        }
        let mut rd = fs::read_dir(dir).await?;
        while let Some(ent) = rd.next_entry().await? {
            if !ent.file_type().await?.is_file() {
                continue;
            }
            let name = ent.file_name().to_string_lossy().into_owned();
            let (id, flags) = split_maildir_filename(&name);
            if id == base_id {
                return Ok((ent.path(), flags, in_cur));
            }
        }
    }
    Err(ChatmailError::storage(format!(
        "message {base_id} not found in {mailbox} for {user}"
    )))
}

/// Apply `+FLAGS (\Seen)` / `+FLAGS (\Deleted)` (Delta Chat core paths).
///
/// Chatmail is a **relay**, not a classical mailbox: messages land in INBOX briefly,
/// core fetches them, then marks `\Deleted` (default `delete_server_after=1` on
/// XCHATMAIL). Deletion is immediate — no long-lived `:2,T` staging.
pub async fn store_add_flags(
    store: &MailboxStore,
    user: &str,
    mailbox: &str,
    base_id: &str,
    add_seen: bool,
    add_deleted: bool,
) -> Result<MaildirFlags> {
    let (path, mut flags, in_cur) = locate_message(store, user, mailbox, base_id).await?;
    if in_cur {
        flags.seen = true;
    }
    if add_seen {
        flags.seen = true;
    }
    if add_deleted {
        flags.deleted = true;
        fs::remove_file(&path).await?;
        store.invalidate_mailbox_listing(user, mailbox);
        return Ok(flags);
    }
    let paths = store.maildir_for_mailbox(user, mailbox);
    let new_name = maildir_filename(base_id, &flags);
    let target_dir = if flags.seen { &paths.cur } else { &paths.new };
    fs::create_dir_all(target_dir).await?;
    let target = target_dir.join(&new_name);
    if path != target {
        fs::rename(&path, &target).await?;
    } else if in_cur != flags.seen {
        let other = if flags.seen {
            paths.new.join(&new_name)
        } else {
            paths.cur.join(&new_name)
        };
        fs::rename(&path, &other).await?;
    }
    store.invalidate_mailbox_listing(user, mailbox);
    Ok(flags)
}

pub async fn move_message(
    store: &MailboxStore,
    user: &str,
    from_mailbox: &str,
    to_mailbox: &str,
    base_id: &str,
) -> Result<()> {
    store.init_mailbox_dir(user, to_mailbox).await?;
    let (path, flags, _) = locate_message(store, user, from_mailbox, base_id).await?;
    let to_paths = store.maildir_for_mailbox(user, to_mailbox);
    let name = maildir_filename(base_id, &flags);
    let target_dir = if flags.seen {
        &to_paths.cur
    } else {
        &to_paths.new
    };
    fs::create_dir_all(target_dir).await?;
    fs::rename(&path, target_dir.join(&name)).await?;
    store.fsync().commit_directory(target_dir).await?;
    store.invalidate_mailbox_listing(user, from_mailbox);
    store.invalidate_mailbox_listing(user, to_mailbox);
    Ok(())
}

pub async fn copy_message(
    store: &MailboxStore,
    user: &str,
    from_mailbox: &str,
    to_mailbox: &str,
    base_id: &str,
) -> Result<String> {
    store.init_mailbox_dir(user, to_mailbox).await?;
    let body = super::read_blob(store, user, from_mailbox, base_id).await?;
    let new_id = uuid::Uuid::new_v4().to_string();
    write_message(
        store,
        user,
        to_mailbox,
        &new_id,
        &body,
        MaildirFlags::default(),
    )
    .await?;
    Ok(new_id)
}

async fn write_message(
    store: &MailboxStore,
    user: &str,
    mailbox: &str,
    base_id: &str,
    body: &[u8],
    flags: MaildirFlags,
) -> Result<PathBuf> {
    let paths = store.init_mailbox_dir(user, mailbox).await?;
    let name = maildir_filename(base_id, &flags);
    let tmp_path = paths.tmp.join(&name);
    let target_dir = if flags.seen { &paths.cur } else { &paths.new };
    let final_path = target_dir.join(&name);

    let mut file = fs::File::create(&tmp_path).await?;
    tokio::io::AsyncWriteExt::write_all(&mut file, body).await?;
    store.fsync().sync_file_data(&mut file).await?;
    fs::rename(&tmp_path, &final_path).await?;
    store.fsync().commit_directory(target_dir).await?;
    store.invalidate_mailbox_listing(user, mailbox);
    Ok(final_path)
}

/// Remove messages marked `\Deleted` (maildir `:2,T`).
pub async fn expunge_deleted(store: &MailboxStore, user: &str, mailbox: &str) -> Result<usize> {
    let paths = store.maildir_for_mailbox(user, mailbox);
    let mut removed = 0usize;
    for dir in [&paths.new, &paths.cur] {
        if !dir.exists() {
            continue;
        }
        let mut rd = fs::read_dir(dir).await?;
        while let Some(ent) = rd.next_entry().await? {
            if !ent.file_type().await?.is_file() {
                continue;
            }
            let name = ent.file_name().to_string_lossy().into_owned();
            let (_, flags) = split_maildir_filename(&name);
            if flags.deleted {
                fs::remove_file(ent.path()).await?;
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write_blob;

    #[test]
    fn maildir_flag_roundtrip() {
        let flags = MaildirFlags {
            seen: true,
            deleted: true,
        };
        let name = maildir_filename("abc", &flags);
        assert_eq!(name, "abc:2,ST");
        let (base, parsed) = split_maildir_filename(&name);
        assert_eq!(base, "abc");
        assert_eq!(parsed, flags);
    }

    /// P11-UT14: list_mailbox_messages serves cached listing when mtimes unchanged.
    #[tokio::test]
    async fn p11_ut14_list_mailbox_messages_uses_mtime_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        write_blob(&store, "u@test", "m1", b"x").await.unwrap();

        let first = list_mailbox_messages(&store, "u@test", "INBOX")
            .await
            .unwrap();
        assert_eq!(first.len(), 1);

        // Second call hits cache (same dir mtimes).
        let second = list_mailbox_messages(&store, "u@test", "INBOX")
            .await
            .unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].base_id, first[0].base_id);
    }

    /// P11-UT23: listing cache is invalidated after a second write.
    #[tokio::test]
    async fn p11_ut23_list_cache_invalidated_after_write() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        write_blob(&store, "u@test", "m1", b"a").await.unwrap();
        assert_eq!(
            list_mailbox_messages(&store, "u@test", "INBOX")
                .await
                .unwrap()
                .len(),
            1
        );

        write_blob(&store, "u@test", "m2", b"b").await.unwrap();
        let after = list_mailbox_messages(&store, "u@test", "INBOX")
            .await
            .unwrap();
        assert_eq!(after.len(), 2);
    }

    /// Issue #115: concurrent delivery while listing must not leave IMAP serving a permanently
    /// incomplete cached listing (generation-guarded store + invalidate on write).
    #[tokio::test]
    async fn issue115_concurrent_deliver_and_list_never_loses_message() {
        let tmp = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(MailboxStore::new(tmp.path()));

        // Seed one message so the cache is warm.
        write_blob(&store, "u@test", "seed", b"seed").await.unwrap();
        assert_eq!(
            list_mailbox_messages(&store, "u@test", "INBOX")
                .await
                .unwrap()
                .len(),
            1
        );

        let store_w = store.clone();
        let writer = tokio::spawn(async move {
            for i in 0..20 {
                write_blob(&store_w, "u@test", &format!("m{i}"), b"body")
                    .await
                    .unwrap();
            }
        });
        let store_r = store.clone();
        let reader = tokio::spawn(async move {
            for _ in 0..40 {
                let _ = list_mailbox_messages(&store_r, "u@test", "INBOX").await;
                tokio::task::yield_now().await;
            }
        });
        writer.await.unwrap();
        reader.await.unwrap();

        let final_list = list_mailbox_messages(&store, "u@test", "INBOX")
            .await
            .unwrap();
        assert_eq!(
            final_list.len(),
            21,
            "all delivered messages must be visible after concurrent list; got {:?}",
            final_list
                .iter()
                .map(|m| m.base_id.as_str())
                .collect::<Vec<_>>()
        );
    }
}
