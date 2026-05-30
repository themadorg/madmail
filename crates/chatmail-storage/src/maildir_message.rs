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

use crate::maildir::{fsync_dir, MailboxStore};

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

/// List messages in a mailbox, oldest first (stable UIDs = 1..N).
pub async fn list_mailbox_messages(
    store: &MailboxStore,
    user: &str,
    mailbox: &str,
) -> Result<Vec<StoredMessage>> {
    let paths = store.maildir_for_mailbox(user, mailbox);
    let mut items = Vec::new();
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
            let (base_id, mut flags) = split_maildir_filename(&filename);
            if in_cur && !flags.seen {
                flags.seen = true;
            }
            let meta = ent.metadata().await?;
            items.push((
                meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
                StoredMessage {
                    base_id: base_id.to_string(),
                    filename,
                    size: meta.len(),
                    internal_date: meta.modified().unwrap_or_else(|_| SystemTime::now()),
                    flags,
                },
            ));
        }
    }
    items.sort_by_key(|(mtime, _)| *mtime);
    Ok(items.into_iter().map(|(_, m)| m).collect())
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
    fsync_dir(target_dir).await?;
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
    file.sync_data().await?;
    fs::rename(&tmp_path, &final_path).await?;
    // Make the directory entry durable before the caller notifies clients (APPEND / COPY).
    fsync_dir(target_dir).await?;
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
}
