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

//! External blob store abstraction (Go madmail `ExternalStore` + `FSStore` parity).
//!
//! Go madmail keeps message bodies in an `ExternalStore` (keyed blobs) separate from the metadata
//! DB, with `Link` for cheap multi-recipient fan-out and explicit `Sync` for durability. This
//! module introduces the same seam in madmail-v2 as a trait so the body backend can evolve
//! (compression, object storage, ranges) without touching the SMTP/IMAP layers. The default and
//! only backend today is [`FsStore`], which delegates to the existing maildir blob functions — so
//! behaviour is unchanged; this is the foundation the analysis called for ("introduce behind a
//! trait; keep current maildir path as the default").

use async_trait::async_trait;
use chatmail_types::Result;

use crate::blob::{
    delete_blob, link_into_inbox, read_blob, read_blob_known, read_blob_range_known,
    write_blob_mailbox,
};
use crate::maildir::MailboxStore;

/// Address of a body blob within the store: which user's mailbox and the message id / filename.
#[derive(Debug, Clone)]
pub struct ExternalKey {
    pub user: String,
    pub mailbox: String,
    /// The maildir base id (delivery uses the raw msg_id as the `new/` filename).
    pub msg_id: String,
}

impl ExternalKey {
    pub fn new(
        user: impl Into<String>,
        mailbox: impl Into<String>,
        msg_id: impl Into<String>,
    ) -> Self {
        Self {
            user: user.into(),
            mailbox: mailbox.into(),
            msg_id: msg_id.into(),
        }
    }

    /// Convenience for INBOX-addressed bodies (the common delivery target).
    pub fn inbox(user: impl Into<String>, msg_id: impl Into<String>) -> Self {
        Self::new(user, "INBOX", msg_id)
    }
}

/// A keyed, durable body store with cheap linking for multi-recipient fan-out.
#[async_trait]
pub trait ExternalStore: Send + Sync {
    /// Durably write `body` at `key` (content fsync + directory fsync via the backend).
    async fn put(&self, key: &ExternalKey, body: &[u8]) -> Result<()>;

    /// Read the full body at `key`.
    async fn get(&self, key: &ExternalKey) -> Result<Vec<u8>>;

    /// Read a byte range `[offset, offset+count)` (or to EOF when `count` is `None`) without
    /// materializing the whole body. Returns `None` if the blob is not found.
    async fn get_range(
        &self,
        key: &ExternalKey,
        offset: u64,
        count: Option<u64>,
    ) -> Result<Option<Vec<u8>>>;

    /// Link `src`'s body into `dest` (hardlink, falling back to copy across devices). Both end up
    /// referencing the same content for as long as either exists — the efficient fan-out path.
    async fn link(&self, src: &ExternalKey, dest: &ExternalKey) -> Result<()>;

    /// Remove the blob at `key` (idempotent at the caller's discretion).
    async fn delete(&self, key: &ExternalKey) -> Result<()>;
}

/// Filesystem-backed [`ExternalStore`] over the maildir layout (the production default).
#[derive(Debug, Clone)]
pub struct FsStore {
    store: MailboxStore,
}

impl FsStore {
    pub fn new(store: MailboxStore) -> Self {
        Self { store }
    }

    pub fn mailbox_store(&self) -> &MailboxStore {
        &self.store
    }
}

#[async_trait]
impl ExternalStore for FsStore {
    async fn put(&self, key: &ExternalKey, body: &[u8]) -> Result<()> {
        write_blob_mailbox(&self.store, &key.user, &key.mailbox, &key.msg_id, body).await?;
        Ok(())
    }

    async fn get(&self, key: &ExternalKey) -> Result<Vec<u8>> {
        read_blob(&self.store, &key.user, &key.mailbox, &key.msg_id).await
    }

    async fn get_range(
        &self,
        key: &ExternalKey,
        offset: u64,
        count: Option<u64>,
    ) -> Result<Option<Vec<u8>>> {
        // Delivery writes the body as `new/<msg_id>` (no maildir flag suffix), so the msg_id is the
        // filename for the freshly-delivered blobs this store manages. Fall back to a full read +
        // slice for entries that have since been renamed (flag changes).
        if let Some(bytes) = read_blob_range_known(
            &self.store,
            &key.user,
            &key.mailbox,
            &key.msg_id,
            offset,
            count,
        )
        .await?
        {
            return Ok(Some(bytes));
        }
        if read_blob_known(&self.store, &key.user, &key.mailbox, &key.msg_id)
            .await?
            .is_none()
        {
            // Try a scanning read; if even that fails the blob truly does not exist.
            match read_blob(&self.store, &key.user, &key.mailbox, &key.msg_id).await {
                Ok(full) => {
                    let start = (offset as usize).min(full.len());
                    let end = match count {
                        Some(c) => (start + c as usize).min(full.len()),
                        None => full.len(),
                    };
                    return Ok(Some(full[start..end].to_vec()));
                }
                Err(_) => return Ok(None),
            }
        }
        Ok(None)
    }

    async fn link(&self, src: &ExternalKey, dest: &ExternalKey) -> Result<()> {
        let canonical = self
            .store
            .maildir_for_mailbox(&src.user, &src.mailbox)
            .new
            .join(&src.msg_id);
        link_into_inbox(&self.store, &dest.user, &dest.msg_id, &canonical).await?;
        Ok(())
    }

    async fn delete(&self, key: &ExternalKey) -> Result<()> {
        delete_blob(&self.store, &key.user, &key.msg_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P10-UT06: FsStore put/get/get_range/link/delete behave like the maildir blob path and link
    /// shares a single inode for fan-out.
    #[tokio::test]
    async fn p10_ut06_fs_store_roundtrip_and_link() {
        let tmp = tempfile::tempdir().unwrap();
        let fs = FsStore::new(MailboxStore::new(tmp.path()));
        let body = b"From: a@b.test\r\n\r\nbinary\xff\x00payload".to_vec();

        let alice = ExternalKey::inbox("alice@test", "msg-a");
        fs.put(&alice, &body).await.unwrap();
        assert_eq!(fs.get(&alice).await.unwrap(), body);

        // Range read.
        let range = fs.get_range(&alice, 0, Some(4)).await.unwrap().unwrap();
        assert_eq!(range, &body[0..4]);

        // Link to a second recipient shares the inode (cheap fan-out).
        let bob = ExternalKey::inbox("bob@test", "msg-b");
        fs.link(&alice, &bob).await.unwrap();
        assert_eq!(fs.get(&bob).await.unwrap(), body);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let store = fs.mailbox_store();
            let pa = store
                .maildir_for_mailbox("alice@test", "INBOX")
                .new
                .join("msg-a");
            let pb = store
                .maildir_for_mailbox("bob@test", "INBOX")
                .new
                .join("msg-b");
            assert_eq!(
                std::fs::metadata(&pa).unwrap().ino(),
                std::fs::metadata(&pb).unwrap().ino(),
                "link shares one inode"
            );
        }

        // Missing blob → get_range None.
        let ghost = ExternalKey::inbox("ghost@test", "nope");
        assert!(fs.get_range(&ghost, 0, None).await.unwrap().is_none());

        // Delete removes alice's copy; bob still has the (now unlinked) content.
        fs.delete(&alice).await.unwrap();
        assert!(fs.get(&alice).await.is_err());
        assert_eq!(fs.get(&bob).await.unwrap(), body);
    }

    /// FsStore is object-safe (usable as `Arc<dyn ExternalStore>` for config-selectable backends).
    #[tokio::test]
    async fn fs_store_is_object_safe() {
        let tmp = tempfile::tempdir().unwrap();
        let store: std::sync::Arc<dyn ExternalStore> =
            std::sync::Arc::new(FsStore::new(MailboxStore::new(tmp.path())));
        let key = ExternalKey::inbox("u@test", "m1");
        store.put(&key, b"hi").await.unwrap();
        assert_eq!(store.get(&key).await.unwrap(), b"hi");
    }
}
