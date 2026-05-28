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

use chatmail_types::Result;

use crate::maildir::MailboxStore;

/// One message in the user's INBOX (stable UID assignment by scan order).
#[derive(Debug, Clone)]
pub struct InboxEntry {
    pub uid: u32,
    pub msg_id: String,
    pub size: u64,
}

/// List all messages in `new/` and `cur/` with monotonically assigned UIDs (1-based, oldest→newest).
pub async fn list_inbox(store: &MailboxStore, user: &str) -> Result<Vec<InboxEntry>> {
    let paths = store.maildir_for_user(user);
    let mut items = Vec::new();
    for dir in [&paths.new, &paths.cur] {
        if !dir.exists() {
            continue;
        }
        let mut rd = tokio::fs::read_dir(dir).await?;
        while let Some(ent) = rd.next_entry().await? {
            if ent.file_type().await?.is_file() {
                let msg_id = ent.file_name().to_string_lossy().into_owned();
                let meta = ent.metadata().await?;
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                items.push((mtime, InboxEntry {
                    uid: 0,
                    msg_id,
                    size: meta.len(),
                }));
            }
        }
    }
    items.sort_by_key(|(mtime, _)| *mtime);
    Ok(items
        .into_iter()
        .enumerate()
        .map(|(i, (_, mut e))| {
            e.uid = (i + 1) as u32;
            e
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{write_blob, MailboxStore};

    #[tokio::test]
    async fn list_inbox_assigns_uids_oldest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        store.init_user_dir("u@test").await.unwrap();
        write_blob(&store, "u@test", "first", b"1").await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        write_blob(&store, "u@test", "second", b"2").await.unwrap();

        let list = list_inbox(&store, "u@test").await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].msg_id, "first");
        assert_eq!(list[0].uid, 1);
        assert_eq!(list[0].size, 1);
        assert_eq!(list[1].msg_id, "second");
        assert_eq!(list[1].uid, 2);
    }

    #[tokio::test]
    async fn list_inbox_includes_new_and_cur() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let paths = store.init_user_dir("u@test").await.unwrap();
        write_blob(&store, "u@test", "unread", b"u").await.unwrap();
        write_blob(&store, "u@test", "read", b"r").await.unwrap();
        tokio::fs::rename(paths.new.join("read"), paths.cur.join("read"))
            .await
            .unwrap();

        let list = list_inbox(&store, "u@test").await.unwrap();
        assert_eq!(list.len(), 2);
        let ids: Vec<_> = list.iter().map(|e| e.msg_id.as_str()).collect();
        assert!(ids.contains(&"unread"));
        assert!(ids.contains(&"read"));
    }

    #[tokio::test]
    async fn list_inbox_empty_when_no_maildir() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let list = list_inbox(&store, "nobody@test").await.unwrap();
        assert!(list.is_empty());
    }
}
