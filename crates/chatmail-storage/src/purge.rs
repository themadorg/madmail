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

//! Maildir purge helpers (Madmail `/admin/queue` + `state_dir/messages/` equivalents).

use std::path::Path;
use std::time::{Duration, SystemTime};

use chatmail_types::Result;

use crate::maildir::MailboxStore;

/// Delete all message files under `{state_dir}/mail/` (`new/`, `cur/`, `tmp/`).
pub async fn purge_all_mail_blobs(store: &MailboxStore) -> Result<usize> {
    let mail_root = store.state_dir().join("mail");
    purge_tree_files(&mail_root, None).await
}

/// Delete message files older than `retention` (by filesystem mtime).
pub async fn purge_mail_blobs_older(store: &MailboxStore, retention: Duration) -> Result<usize> {
    let mail_root = store.state_dir().join("mail");
    let cutoff = SystemTime::now()
        .checked_sub(retention)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    purge_tree_files(&mail_root, Some(cutoff)).await
}

/// Delete all messages for one user (all mailboxes under `mail/{user}/`).
pub async fn purge_user_messages(store: &MailboxStore, user: &str) -> Result<usize> {
    let user_root = store.state_dir().join("mail");
    let mut deleted = 0usize;
    let mut rd = match tokio::fs::read_dir(&user_root).await {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };
    while let Some(ent) = rd.next_entry().await? {
        if !ent.file_type().await?.is_dir() {
            continue;
        }
        let name = ent.file_name().to_string_lossy().into_owned();
        if user_matches_dir(user, &name) {
            deleted += purge_maildir_subdirs(&ent.path(), None).await?;
        }
    }
    Ok(deleted)
}

/// Delete files in `cur/` (maildir “seen” / opened messages).
pub async fn purge_read_messages(store: &MailboxStore) -> Result<usize> {
    purge_maildir_dirs(store, |name| name == "cur", None).await
}

/// Delete unread (`new/`) messages older than `retention` (Madmail `PruneUnreadIMAPMsgs`).
pub async fn prune_unread_older(store: &MailboxStore, retention: Duration) -> Result<usize> {
    let cutoff = SystemTime::now()
        .checked_sub(retention)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    purge_maildir_dirs(store, |name| name == "new", Some(cutoff)).await
}

fn user_matches_dir(user: &str, dir_name: &str) -> bool {
    let sanitized = user.replace(['/', '\\'], "_");
    dir_name == sanitized || dir_name == user
}

async fn purge_maildir_dirs<F>(
    store: &MailboxStore,
    dir_filter: F,
    cutoff: Option<SystemTime>,
) -> Result<usize>
where
    F: Fn(&str) -> bool,
{
    let mail_root = store.state_dir().join("mail");
    let mut deleted = 0usize;
    let mut stack = vec![mail_root];
    while let Some(dir) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        while let Some(ent) = rd.next_entry().await? {
            let path = ent.path();
            let ft = ent.file_type().await?;
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            let parent = path.parent().and_then(|p| p.file_name());
            let Some(parent_name) = parent.map(|n| n.to_string_lossy()) else {
                continue;
            };
            if dir_filter(parent_name.as_ref()) && file_older_than(&path, cutoff).await? {
                tokio::fs::remove_file(&path).await?;
                deleted += 1;
            }
        }
    }
    Ok(deleted)
}

async fn purge_maildir_subdirs(root: &Path, cutoff: Option<SystemTime>) -> Result<usize> {
    let mut deleted = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        while let Some(ent) = rd.next_entry().await? {
            let path = ent.path();
            let ft = ent.file_type().await?;
            if ft.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str());
                if matches!(name, Some("new" | "cur" | "tmp")) {
                    deleted += purge_dir_files(&path, cutoff).await?;
                } else {
                    stack.push(path);
                }
                continue;
            }
        }
    }
    Ok(deleted)
}

async fn purge_tree_files(root: &Path, cutoff: Option<SystemTime>) -> Result<usize> {
    if !root.exists() {
        return Ok(0);
    }
    let mut deleted = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        while let Some(ent) = rd.next_entry().await? {
            let path = ent.path();
            let ft = ent.file_type().await?;
            if ft.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str());
                if matches!(name, Some("new" | "cur" | "tmp")) {
                    deleted += purge_dir_files(&path, cutoff).await?;
                } else {
                    stack.push(path);
                }
            }
        }
    }
    Ok(deleted)
}

async fn purge_dir_files(dir: &Path, cutoff: Option<SystemTime>) -> Result<usize> {
    let mut deleted = 0usize;
    let mut rd = match tokio::fs::read_dir(dir).await {
        Ok(r) => r,
        Err(_) => return Ok(0),
    };
    while let Some(ent) = rd.next_entry().await? {
        if !ent.file_type().await?.is_file() {
            continue;
        }
        let path = ent.path();
        if file_older_than(&path, cutoff).await? {
            tokio::fs::remove_file(&path).await?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

async fn file_older_than(path: &Path, cutoff: Option<SystemTime>) -> Result<bool> {
    let Some(cutoff) = cutoff else {
        return Ok(true);
    };
    let meta = tokio::fs::metadata(path).await?;
    Ok(meta.modified().map(|m| m < cutoff).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use filetime::{set_file_mtime, FileTime};

    use super::*;
    use crate::{list_inbox, write_blob, MailboxStore};

    fn touch_unix_epoch(path: &Path) {
        set_file_mtime(path, FileTime::from_unix_time(1, 0)).unwrap();
    }

    async fn deliver(store: &MailboxStore, user: &str, msg_id: &str, body: &[u8]) {
        store.init_user_dir(user).await.unwrap();
        write_blob(store, user, msg_id, body).await.unwrap();
    }

    #[tokio::test]
    async fn purge_all_mail_blobs_removes_new_and_cur() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        deliver(&store, "a@test", "m1", b"x").await;
        deliver(&store, "b@test", "m2", b"y").await;

        let n = purge_all_mail_blobs(&store).await.unwrap();
        assert_eq!(n, 2);
        assert!(list_inbox(&store, "a@test").await.unwrap().is_empty());
        assert!(list_inbox(&store, "b@test").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn purge_user_messages_only_targets_one_user() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        deliver(&store, "keep@test", "k", b"k").await;
        deliver(&store, "drop@test", "d", b"d").await;

        let n = purge_user_messages(&store, "drop@test").await.unwrap();
        assert_eq!(n, 1);
        assert_eq!(list_inbox(&store, "keep@test").await.unwrap().len(), 1);
        assert!(list_inbox(&store, "drop@test").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn purge_user_messages_matches_sanitized_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        deliver(&store, "user/name@test", "m", b"x").await;

        let n = purge_user_messages(&store, "user/name@test").await.unwrap();
        assert_eq!(n, 1);
        assert!(list_inbox(&store, "user/name@test").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn purge_read_messages_only_deletes_cur() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let paths = store.init_user_dir("u@test").await.unwrap();
        write_blob(&store, "u@test", "new-msg", b"n").await.unwrap();
        write_blob(&store, "u@test", "old-read", b"r").await.unwrap();
        tokio::fs::rename(paths.new.join("old-read"), paths.cur.join("old-read"))
            .await
            .unwrap();

        let n = purge_read_messages(&store).await.unwrap();
        assert_eq!(n, 1);
        let left = list_inbox(&store, "u@test").await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].msg_id, "new-msg");
    }

    #[tokio::test]
    async fn prune_unread_older_respects_retention() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let paths = store.init_user_dir("u@test").await.unwrap();
        write_blob(&store, "u@test", "stale", b"s").await.unwrap();
        write_blob(&store, "u@test", "fresh", b"f").await.unwrap();
        touch_unix_epoch(&paths.new.join("stale"));

        let n = prune_unread_older(&store, Duration::from_secs(3600))
            .await
            .unwrap();
        assert_eq!(n, 1);
        let left = list_inbox(&store, "u@test").await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].msg_id, "fresh");
    }

    #[tokio::test]
    async fn purge_mail_blobs_older_respects_retention() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let paths = store.init_user_dir("u@test").await.unwrap();
        write_blob(&store, "u@test", "old", b"o").await.unwrap();
        write_blob(&store, "u@test", "new", b"n").await.unwrap();
        touch_unix_epoch(&paths.new.join("old"));

        let n = purge_mail_blobs_older(&store, Duration::from_secs(3600))
            .await
            .unwrap();
        assert_eq!(n, 1);
        let left = list_inbox(&store, "u@test").await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].msg_id, "new");
    }
}
