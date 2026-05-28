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
use tokio::io::AsyncWriteExt;

use crate::maildir::MailboxStore;
use crate::maildir_message::split_maildir_filename;

/// Deliver one message to multiple local users with a single on-disk body (hardlinks).
///
/// Madmail writes the blob once, then `os.Link`s for additional recipients on the same server.
/// The first recipient gets a normal atomic write; further recipients get hardlinks into their
/// maildir `new/` (fallback: full copy if hardlink fails, e.g. cross-device).
pub async fn deliver_local_messages(
    store: &MailboxStore,
    deliveries: &[(String, String)],
    body: &[u8],
) -> Result<()> {
    if deliveries.is_empty() {
        return Ok(());
    }
    if deliveries.len() == 1 {
        let (user, msg_id) = &deliveries[0];
        write_blob(store, user, msg_id, body).await?;
        return Ok(());
    }

    let (first_user, first_msg_id) = &deliveries[0];
    let canonical = write_blob(store, first_user, first_msg_id, body).await?;

    for (user, msg_id) in deliveries.iter().skip(1) {
        link_into_inbox(store, user, msg_id, &canonical).await?;
    }
    Ok(())
}

async fn link_into_inbox(
    store: &MailboxStore,
    user: &str,
    msg_id: &str,
    canonical: &Path,
) -> Result<PathBuf> {
    store.init_mailbox_dir(user, "INBOX").await?;
    let dest = store
        .maildir_for_mailbox(user, "INBOX")
        .new
        .join(msg_id);
    if dest.exists() {
        return Err(ChatmailError::storage(format!(
            "message {msg_id} already exists for {user}"
        )));
    }
    match tokio::fs::hard_link(canonical, &dest).await {
        Ok(()) => Ok(dest),
        Err(e) if is_cross_device_link(&e) => {
            tokio::fs::copy(canonical, &dest).await?;
            Ok(dest)
        }
        Err(e) => Err(ChatmailError::from(e)),
    }
}

#[cfg(unix)]
fn is_cross_device_link(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(18) // EXDEV
}

#[cfg(not(unix))]
fn is_cross_device_link(e: &std::io::Error) -> bool {
    let _ = e;
    false
}

/// Write a message blob atomically to INBOX (`tmp/` → fsync → rename → `new/`).
pub async fn write_blob(
    store: &MailboxStore,
    user: &str,
    msg_id: &str,
    body: &[u8],
) -> Result<PathBuf> {
    write_blob_mailbox(store, user, "INBOX", msg_id, body).await
}

/// Write a message blob to a specific mailbox.
pub async fn write_blob_mailbox(
    store: &MailboxStore,
    user: &str,
    mailbox: &str,
    msg_id: &str,
    body: &[u8],
) -> Result<PathBuf> {
    let paths = store.init_mailbox_dir(user, mailbox).await?;
    let tmp_path = paths.tmp.join(msg_id);
    let new_path = paths.new.join(msg_id);

    let mut file = tokio::fs::File::create(&tmp_path).await?;
    file.write_all(body).await?;
    file.sync_data().await?;

    tokio::fs::rename(&tmp_path, &new_path).await?;
    Ok(new_path)
}

/// Read a blob from `new/` or `cur/`.
pub async fn read_blob(
    store: &MailboxStore,
    user: &str,
    mailbox: &str,
    msg_id: &str,
) -> Result<Vec<u8>> {
    let paths = store.maildir_for_mailbox(user, mailbox);
    for dir in [&paths.new, &paths.cur] {
        if !dir.exists() {
            continue;
        }
        let mut rd = tokio::fs::read_dir(dir).await?;
        while let Some(ent) = rd.next_entry().await? {
            if !ent.file_type().await?.is_file() {
                continue;
            }
            let name = ent.file_name().to_string_lossy().into_owned();
            let (base, _) = split_maildir_filename(&name);
            if base == msg_id {
                return Ok(tokio::fs::read(ent.path()).await?);
            }
        }
    }
    Err(ChatmailError::storage(format!(
        "message {msg_id} not found for {user}"
    )))
}

/// Delete a blob from `new/` or `cur/`.
pub async fn delete_blob(store: &MailboxStore, user: &str, msg_id: &str) -> Result<()> {
    let paths = store.maildir_for_user(user);
    for dir in [&paths.new, &paths.cur] {
        if !dir.exists() {
            continue;
        }
        let mut rd = tokio::fs::read_dir(dir).await?;
        while let Some(ent) = rd.next_entry().await? {
            if !ent.file_type().await?.is_file() {
                continue;
            }
            let name = ent.file_name().to_string_lossy().into_owned();
            let (base, _) = split_maildir_filename(&name);
            if base == msg_id {
                tokio::fs::remove_file(ent.path()).await?;
                return Ok(());
            }
        }
    }
    Err(ChatmailError::storage(format!(
        "message {msg_id} not found for {user}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P2-UT02: Atomic delivery via tmp → new.
    #[tokio::test]
    async fn p2_ut02_test_atomic_write_blob() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let body = b"From: a@b.test\r\nSubject: hi\r\n\r\nPGP body";

        let path = write_blob(&store, "user@example.org", "msg-001", body)
            .await
            .unwrap();

        assert!(path.ends_with("new/msg-001"));
        assert!(!store
            .maildir_for_user("user@example.org")
            .tmp
            .join("msg-001")
            .exists());

        let read = read_blob(&store, "user@example.org", "INBOX", "msg-001")
            .await
            .unwrap();
        assert_eq!(read, body);
    }

    /// Multi-recipient local delivery shares one on-disk inode via hardlinks.
    #[tokio::test]
    async fn deliver_local_messages_hardlinks_body() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let body = b"From: a@b.test\r\nSubject: broadcast\r\n\r\nPGP body";

        let deliveries = [
            ("alice@test".to_string(), "msg-a".to_string()),
            ("bob@test".to_string(), "msg-b".to_string()),
            ("carol@test".to_string(), "msg-c".to_string()),
        ];
        deliver_local_messages(&store, &deliveries, body)
            .await
            .unwrap();

        let mut paths = Vec::new();
        for (user, msg_id) in &deliveries {
            let read = read_blob(&store, user, "INBOX", msg_id).await.unwrap();
            assert_eq!(read, body);
            paths.push(
                store
                    .maildir_for_mailbox(user, "INBOX")
                    .new
                    .join(msg_id),
            );
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let ino = std::fs::metadata(&paths[0]).unwrap().ino();
            for path in &paths[1..] {
                assert_eq!(std::fs::metadata(path).unwrap().ino(), ino);
            }
            assert!(std::fs::metadata(&paths[0]).unwrap().nlink() >= 3);
        }
    }
}
