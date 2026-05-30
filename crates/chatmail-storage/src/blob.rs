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

use crate::maildir::{fsync_dir, MailboxStore};
use crate::maildir_message::split_maildir_filename;

/// Per-recipient result of a local fan-out delivery.
///
/// Group media (a single SMTP transaction fanning a multi-MB photo/video to 60 recipients) widens
/// the window in which a single recipient's write/link can fail (quota at link time, ENOSPC, an
/// `EEXIST` race, cross-device weirdness). The previous all-or-nothing `?` meant one such failure
/// dropped the whole message for *everyone* (and notified no one), while leaving orphan files on
/// disk for the recipients written before the failure. This mirrors Go madmail's `PartialDelivery`
/// model: deliver to as many recipients as possible and report exactly who succeeded so the caller
/// notifies only durably-written recipients.
#[derive(Debug, Default)]
pub struct DeliveryOutcome {
    /// Recipients whose body is durably on disk; safe to notify.
    pub delivered: Vec<(String, String)>,
    /// Recipients that failed, with the error rendered for logging.
    pub failed: Vec<(String, String, String)>,
}

impl DeliveryOutcome {
    pub fn all_failed(&self) -> bool {
        self.delivered.is_empty() && !self.failed.is_empty()
    }
}

/// Deliver one message to multiple local users with a single on-disk body (hardlinks).
///
/// Madmail writes the blob once, then `os.Link`s for additional recipients on the same server.
/// One recipient gets a normal atomic write to establish the canonical inode; further recipients
/// get hardlinks into their maildir `new/` (fallback: full copy if hardlink fails, e.g.
/// cross-device). Failures are tracked per recipient rather than aborting the whole fan-out — see
/// [`DeliveryOutcome`]. An `Err` is only returned when *no* recipient could be written at all
/// (so the SMTP/queue caller can surface a transaction-level failure).
pub async fn deliver_local_messages(
    store: &MailboxStore,
    deliveries: &[(String, String)],
    body: &[u8],
) -> Result<DeliveryOutcome> {
    let mut outcome = DeliveryOutcome::default();
    if deliveries.is_empty() {
        return Ok(outcome);
    }

    // Establish the canonical on-disk body via the first recipient that can be written. If the
    // first write fails (e.g. that user's quota/disk), fall through to the next candidate instead
    // of failing the entire group.
    let mut canonical: Option<PathBuf> = None;
    let mut linked_from = 0usize;
    for (idx, (user, msg_id)) in deliveries.iter().enumerate() {
        match write_blob(store, user, msg_id, body).await {
            Ok(path) => {
                canonical = Some(path);
                outcome.delivered.push((user.clone(), msg_id.clone()));
                linked_from = idx + 1;
                break;
            }
            Err(e) => {
                outcome
                    .failed
                    .push((user.clone(), msg_id.clone(), e.to_string()));
            }
        }
    }

    let Some(canonical) = canonical else {
        return Err(ChatmailError::storage(
            "local delivery failed for all recipients",
        ));
    };

    for (user, msg_id) in deliveries.iter().skip(linked_from) {
        match link_into_inbox(store, user, msg_id, &canonical).await {
            Ok(_) => outcome.delivered.push((user.clone(), msg_id.clone())),
            Err(e) => {
                outcome
                    .failed
                    .push((user.clone(), msg_id.clone(), e.to_string()));
            }
        }
    }
    Ok(outcome)
}

async fn link_into_inbox(
    store: &MailboxStore,
    user: &str,
    msg_id: &str,
    canonical: &Path,
) -> Result<PathBuf> {
    store.init_mailbox_dir(user, "INBOX").await?;
    let dest = store.maildir_for_mailbox(user, "INBOX").new.join(msg_id);
    if dest.exists() {
        return Err(ChatmailError::storage(format!(
            "message {msg_id} already exists for {user}"
        )));
    }
    let new_dir = store.maildir_for_mailbox(user, "INBOX").new;
    match tokio::fs::hard_link(canonical, &dest).await {
        Ok(()) => {
            // Make the new directory entry durable before any client is notified.
            fsync_dir(&new_dir).await?;
            Ok(dest)
        }
        Err(e) if is_cross_device_link(&e) => {
            tokio::fs::copy(canonical, &dest).await?;
            fsync_dir(&new_dir).await?;
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
    // fsync the directory so the rename is durable: content survives via sync_data above, but the
    // directory entry must also be flushed or the message can disappear after a crash/reboot.
    fsync_dir(&paths.new).await?;
    Ok(new_path)
}

/// Read a blob whose maildir filename is already known from a prior listing, skipping the full
/// directory scan that [`read_blob`] performs on every body FETCH.
///
/// Under a 60-recipient group media burst, dozens of clients each do header + body FETCHes; the
/// original `read_blob` paid a `read_dir` + linear filename comparison per call (a thundering-herd
/// scan over `new/`+`cur/`). The IMAP listing already discovered each message's exact filename
/// (cached per connection keyed by `inbox_version`), so a body read can open that path directly.
/// Returns `Ok(None)` when the file is not where the listing said (a flag change may have moved it
/// between `new/` and `cur/` since), letting the caller fall back to the scanning [`read_blob`].
pub async fn read_blob_known(
    store: &MailboxStore,
    user: &str,
    mailbox: &str,
    filename: &str,
) -> Result<Option<Vec<u8>>> {
    let paths = store.maildir_for_mailbox(user, mailbox);
    for dir in [&paths.new, &paths.cur] {
        match tokio::fs::read(dir.join(filename)).await {
            Ok(bytes) => return Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(ChatmailError::from(e)),
        }
    }
    Ok(None)
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
        let outcome = deliver_local_messages(&store, &deliveries, body)
            .await
            .unwrap();
        assert_eq!(outcome.delivered.len(), 3);
        assert!(outcome.failed.is_empty());

        let mut paths = Vec::new();
        for (user, msg_id) in &deliveries {
            let read = read_blob(&store, user, "INBOX", msg_id).await.unwrap();
            assert_eq!(read, body);
            paths.push(store.maildir_for_mailbox(user, "INBOX").new.join(msg_id));
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

    /// P10-UT04: a body read by known filename returns the exact bytes and avoids the scan path;
    /// a stale/unknown filename yields `None` so the caller can fall back to scanning.
    #[tokio::test]
    async fn p10_ut04_read_blob_known_direct_and_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let body = b"From: a@b.test\r\n\r\nbinary\xff\x00body";

        write_blob(&store, "u@test", "msg-1", body).await.unwrap();

        // Known filename (delivery writes `new/<msg_id>` with no flag suffix) reads directly.
        let got = read_blob_known(&store, "u@test", "INBOX", "msg-1")
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some(&body[..]));

        // Unknown / stale filename returns None (not an error) so callers fall back to read_blob.
        let missing = read_blob_known(&store, "u@test", "INBOX", "msg-1:2,S")
            .await
            .unwrap();
        assert!(missing.is_none());
    }

    /// P10-UT03: a single recipient failure in the fan-out does not drop the message for the
    /// others; the outcome reports exactly who was (and was not) delivered.
    #[tokio::test]
    async fn p10_ut03_partial_fanout_reports_per_recipient() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let body = b"From: a@b.test\r\nSubject: group media\r\n\r\nPGP body";

        // Force the link for `bob` to fail by pre-creating the destination name (EEXIST path).
        store.init_mailbox_dir("bob@test", "INBOX").await.unwrap();
        let bob_dest = store
            .maildir_for_mailbox("bob@test", "INBOX")
            .new
            .join("msg-b");
        tokio::fs::write(&bob_dest, b"pre-existing").await.unwrap();

        let deliveries = [
            ("alice@test".to_string(), "msg-a".to_string()),
            ("bob@test".to_string(), "msg-b".to_string()),
            ("carol@test".to_string(), "msg-c".to_string()),
        ];
        let outcome = deliver_local_messages(&store, &deliveries, body)
            .await
            .unwrap();

        assert!(!outcome.all_failed());
        let delivered: Vec<&str> = outcome.delivered.iter().map(|(u, _)| u.as_str()).collect();
        assert!(delivered.contains(&"alice@test"));
        assert!(delivered.contains(&"carol@test"));
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].0, "bob@test");

        // Delivered recipients have the real body; the failed one keeps its pre-existing content.
        assert_eq!(
            read_blob(&store, "alice@test", "INBOX", "msg-a")
                .await
                .unwrap(),
            body
        );
        assert_eq!(
            read_blob(&store, "carol@test", "INBOX", "msg-c")
                .await
                .unwrap(),
            body
        );
    }

    /// When no recipient can be written at all, the caller gets a hard error.
    #[tokio::test]
    async fn deliver_local_messages_all_failed_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        let body = b"x";

        store.init_mailbox_dir("solo@test", "INBOX").await.unwrap();
        let dest = store
            .maildir_for_mailbox("solo@test", "INBOX")
            .new
            .join("only");
        // Make the *new* path a directory so the atomic rename onto it fails.
        tokio::fs::create_dir(&dest).await.unwrap();

        let deliveries = [("solo@test".to_string(), "only".to_string())];
        let err = deliver_local_messages(&store, &deliveries, body).await;
        assert!(err.is_err(), "all-failed delivery must surface an error");
    }
}
