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
use std::time::{SystemTime, UNIX_EPOCH};

use chatmail_types::{ChatmailError, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMeta {
    pub id: String,
    pub mail_from: String,
    pub rcpt_to: String,
    pub tries_count: u32,
    /// When the message was first queued (unix secs).
    #[serde(default)]
    pub queued_at_unix: u64,
    pub last_attempt_unix: u64,
    pub next_attempt_unix: u64,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl QueueMeta {
    /// Time the message entered the queue (falls back for pre-migration `.meta` files).
    pub fn effective_queued_at(&self) -> u64 {
        if self.queued_at_unix > 0 {
            self.queued_at_unix
        } else {
            self.last_attempt_unix
        }
    }
}

#[derive(Clone)]
pub struct QueueStore {
    location: PathBuf,
}

impl QueueStore {
    pub fn new(location: PathBuf) -> Self {
        Self { location }
    }

    pub fn location(&self) -> &Path {
        &self.location
    }

    pub async fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.location).await?;
        Ok(())
    }

    pub async fn write_new(
        &self,
        id: &str,
        mail_from: &str,
        rcpt_to: &str,
        body: &[u8],
        next_attempt_unix: u64,
    ) -> Result<()> {
        let now = now_unix();
        let meta = QueueMeta {
            id: id.to_string(),
            mail_from: mail_from.to_string(),
            rcpt_to: rcpt_to.to_string(),
            tries_count: 0,
            queued_at_unix: now,
            last_attempt_unix: 0,
            next_attempt_unix,
            last_error: None,
        };
        self.write_body(id, body).await?;
        self.write_meta(&meta).await?;
        Ok(())
    }

    /// Write one message for many recipients using the madmail single-write +
    /// hard-link fan-out: the body is written to disk **once**, then hard-linked
    /// into every other recipient's entry (same bytes, one inode, no second copy),
    /// and each recipient gets its own small `.meta`. Returns the entry ids in
    /// recipient order.
    ///
    /// Because the entries share one inode, [`Self::remove`] unlinking a delivered
    /// entry just drops one link; the body's bytes live until the last recipient's
    /// entry is removed — the same refcount-by-hard-link that local delivery relies
    /// on.
    ///
    /// Empty `rcpts` returns `Ok([])` without touching the filesystem.
    ///
    /// On any mid-batch failure this method rolls back every entry it created
    /// (bodies, hard links, metas, and temp files) so a failed SMTP DATA does not
    /// leave partial queue state that would be delivered on reload while the client
    /// retries (duplicate risk).
    pub async fn write_shared(
        &self,
        mail_from: &str,
        rcpts: &[String],
        body: &[u8],
        next_attempt_unix: u64,
    ) -> Result<Vec<String>> {
        if rcpts.is_empty() {
            return Ok(Vec::new());
        }

        let now = now_unix();
        let ids: Vec<String> = rcpts
            .iter()
            .map(|_| uuid::Uuid::new_v4().to_string())
            .collect();

        match self
            .write_shared_inner(mail_from, rcpts, body, next_attempt_unix, now, &ids)
            .await
        {
            Ok(()) => Ok(ids),
            Err(e) => {
                self.abort_entries(&ids).await;
                Err(e)
            }
        }
    }

    async fn write_shared_inner(
        &self,
        mail_from: &str,
        rcpts: &[String],
        body: &[u8],
        next_attempt_unix: u64,
        now: u64,
        ids: &[String],
    ) -> Result<()> {
        // 1. Write the body exactly once (the only real copy on disk).
        self.write_body(&ids[0], body).await?;
        let canonical = self.body_path(&ids[0]);

        #[cfg(test)]
        test_failpoints::maybe_fail(test_failpoints::AFTER_BODY)?;

        // 2. Hard-link that single body into every other recipient's entry.
        for id in &ids[1..] {
            fs::hard_link(&canonical, self.body_path(id)).await?;
        }

        #[cfg(test)]
        test_failpoints::maybe_fail(test_failpoints::AFTER_LINKS)?;

        // 3. Write per-recipient meta. Written after the body/links so an entry is
        //    never observable (by reload or the worker) without its body present.
        #[cfg(test)]
        let mut first_meta_written = false;
        for (id, rcpt) in ids.iter().zip(rcpts) {
            let meta = QueueMeta {
                id: id.clone(),
                mail_from: mail_from.to_string(),
                rcpt_to: rcpt.clone(),
                tries_count: 0,
                queued_at_unix: now,
                last_attempt_unix: 0,
                next_attempt_unix,
                last_error: None,
            };
            self.write_meta(&meta).await?;

            #[cfg(test)]
            if !first_meta_written {
                first_meta_written = true;
                test_failpoints::maybe_fail(test_failpoints::AFTER_FIRST_META)?;
            }
        }

        Ok(())
    }

    /// Best-effort cleanup of a partial multi-recipient write (meta, body, temps).
    async fn abort_entries(&self, ids: &[String]) {
        for id in ids {
            let _ = fs::remove_file(self.meta_path(id)).await;
            let _ = fs::remove_file(self.body_path(id)).await;
            let _ = fs::remove_file(self.location.join(format!("{id}.body.new"))).await;
            let _ = fs::remove_file(self.location.join(format!("{id}.meta.new"))).await;
        }
    }

    pub async fn update_meta(&self, meta: &QueueMeta) -> Result<()> {
        self.write_meta(meta).await
    }

    pub async fn load(&self, id: &str) -> Result<(QueueMeta, Vec<u8>)> {
        let meta = self.read_meta(id).await?;
        let body = fs::read(self.body_path(id)).await?;
        Ok((meta, body))
    }

    pub async fn remove(&self, id: &str) {
        let _ = fs::remove_file(self.meta_path(id)).await;
        let _ = fs::remove_file(self.body_path(id)).await;
    }

    pub async fn list_ids(&self) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        let mut rd = match fs::read_dir(&self.location).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ids),
            Err(e) => return Err(e.into()),
        };
        while let Some(ent) = rd.next_entry().await? {
            let name = ent.file_name().to_string_lossy().into_owned();
            if name.ends_with(".meta") {
                ids.push(name.trim_end_matches(".meta").to_string());
            }
        }
        Ok(ids)
    }

    pub async fn count_entries(&self) -> Result<usize> {
        Ok(self.list_ids().await?.len())
    }

    /// Remove all queued outbound messages (`.meta` + `.body`).
    pub async fn purge_all(&self) -> Result<usize> {
        let mut deleted = 0usize;
        for id in self.list_ids().await? {
            self.remove(&id).await;
            deleted += 1;
        }
        Ok(deleted)
    }

    async fn write_body(&self, id: &str, body: &[u8]) -> Result<()> {
        let path = self.body_path(id);
        let tmp = self.location.join(format!("{id}.body.new"));
        let mut f = fs::File::create(&tmp).await?;
        f.write_all(body).await?;
        f.sync_data().await?;
        fs::rename(&tmp, &path).await?;
        Ok(())
    }

    async fn write_meta(&self, meta: &QueueMeta) -> Result<()> {
        let path = self.meta_path(&meta.id);
        let tmp = self.location.join(format!("{}.meta.new", meta.id));
        let data = serde_json::to_vec(meta).map_err(|e| ChatmailError::storage(e.to_string()))?;
        let mut f = fs::File::create(&tmp).await?;
        f.write_all(&data).await?;
        f.sync_data().await?;
        fs::rename(&tmp, &path).await?;
        Ok(())
    }

    pub async fn read_meta(&self, id: &str) -> Result<QueueMeta> {
        let data = fs::read(self.meta_path(id)).await?;
        serde_json::from_slice(&data)
            .map_err(|e| ChatmailError::storage(format!("bad queue meta {id}: {e}")))
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.location.join(format!("{id}.meta"))
    }

    fn body_path(&self, id: &str) -> PathBuf {
        self.location.join(format!("{id}.body"))
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Test-only failpoints for exercising `write_shared` rollback paths.
///
/// Thread-local so parallel tests (each on their own `#[tokio::test]` thread)
/// cannot cross-contaminate.
#[cfg(test)]
pub mod test_failpoints {
    use std::cell::Cell;

    use chatmail_types::{ChatmailError, Result};

    pub const NONE: u8 = 0;
    pub const AFTER_BODY: u8 = 1;
    pub const AFTER_LINKS: u8 = 2;
    pub const AFTER_FIRST_META: u8 = 3;

    thread_local! {
        static FAIL_AT: Cell<u8> = const { Cell::new(NONE) };
    }

    /// Disarms the failpoint when dropped (panic-safe cleanup).
    pub struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            disarm();
        }
    }

    pub fn arm(stage: u8) -> Guard {
        FAIL_AT.with(|c| c.set(stage));
        Guard
    }

    fn disarm() {
        FAIL_AT.with(|c| c.set(NONE));
    }

    pub(super) fn maybe_fail(stage: u8) -> Result<()> {
        FAIL_AT.with(|c| {
            if c.get() == stage {
                c.set(NONE);
                Err(ChatmailError::storage(format!(
                    "test failpoint triggered at stage {stage}"
                )))
            } else {
                Ok(())
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_queue_files(dir: &std::path::Path) -> Vec<String> {
        let mut names = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[tokio::test]
    async fn write_shared_empty_rcpts_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let store = QueueStore::new(dir.path().to_path_buf());
        store.ensure_dir().await.unwrap();

        let ids = store
            .write_shared("a@local.test", &[], b"body", now_unix())
            .await
            .unwrap();
        assert!(ids.is_empty());
        assert!(list_queue_files(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn write_shared_success_one_inode() {
        let dir = tempfile::tempdir().unwrap();
        let store = QueueStore::new(dir.path().to_path_buf());
        store.ensure_dir().await.unwrap();

        let rcpts = vec![
            "u0@remote.test".into(),
            "u1@remote.test".into(),
            "u2@remote.test".into(),
        ];
        let ids = store
            .write_shared("a@local.test", &rcpts, b"shared-body", now_unix())
            .await
            .unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(store.count_entries().await.unwrap(), 3);

        use std::os::unix::fs::MetadataExt;
        let mut inodes = std::collections::HashSet::new();
        for id in &ids {
            let meta = store.read_meta(id).await.unwrap();
            assert!(rcpts.contains(&meta.rcpt_to));
            let body_meta = std::fs::metadata(store.body_path(id)).unwrap();
            inodes.insert(body_meta.ino());
            assert_eq!(body_meta.nlink() as usize, 3);
        }
        assert_eq!(inodes.len(), 1);
    }

    #[tokio::test]
    async fn write_shared_rolls_back_after_body() {
        let dir = tempfile::tempdir().unwrap();
        let store = QueueStore::new(dir.path().to_path_buf());
        store.ensure_dir().await.unwrap();

        let _fp = test_failpoints::arm(test_failpoints::AFTER_BODY);
        let err = store
            .write_shared(
                "a@local.test",
                &["u0@remote.test".into(), "u1@remote.test".into()],
                b"body",
                now_unix(),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("test failpoint"));
        assert_eq!(store.count_entries().await.unwrap(), 0);
        assert!(
            list_queue_files(dir.path()).is_empty(),
            "orphan bodies/temps left: {:?}",
            list_queue_files(dir.path())
        );
    }

    #[tokio::test]
    async fn write_shared_rolls_back_after_links() {
        let dir = tempfile::tempdir().unwrap();
        let store = QueueStore::new(dir.path().to_path_buf());
        store.ensure_dir().await.unwrap();

        let _fp = test_failpoints::arm(test_failpoints::AFTER_LINKS);
        let err = store
            .write_shared(
                "a@local.test",
                &["u0@remote.test".into(), "u1@remote.test".into()],
                b"body",
                now_unix(),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("test failpoint"));
        assert_eq!(store.count_entries().await.unwrap(), 0);
        assert!(
            list_queue_files(dir.path()).is_empty(),
            "orphan bodies/temps left: {:?}",
            list_queue_files(dir.path())
        );
    }

    #[tokio::test]
    async fn write_shared_rolls_back_after_partial_meta() {
        let dir = tempfile::tempdir().unwrap();
        let store = QueueStore::new(dir.path().to_path_buf());
        store.ensure_dir().await.unwrap();

        // Fail after the first recipient's .meta is durable — must still erase it so
        // a client retry cannot double-deliver that recipient.
        let _fp = test_failpoints::arm(test_failpoints::AFTER_FIRST_META);
        let err = store
            .write_shared(
                "a@local.test",
                &[
                    "u0@remote.test".into(),
                    "u1@remote.test".into(),
                    "u2@remote.test".into(),
                ],
                b"body",
                now_unix(),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("test failpoint"));
        assert_eq!(store.count_entries().await.unwrap(), 0);
        assert!(
            list_queue_files(dir.path()).is_empty(),
            "partial complete entries left: {:?}",
            list_queue_files(dir.path())
        );
    }

    #[tokio::test]
    async fn write_shared_single_recipient() {
        let dir = tempfile::tempdir().unwrap();
        let store = QueueStore::new(dir.path().to_path_buf());
        store.ensure_dir().await.unwrap();

        let body = b"single-rcpt-body";
        let ids = store
            .write_shared(
                "a@local.test",
                &["only@remote.test".into()],
                body,
                now_unix(),
            )
            .await
            .unwrap();
        assert_eq!(ids.len(), 1);
        let (meta, loaded) = store.load(&ids[0]).await.unwrap();
        assert_eq!(meta.rcpt_to, "only@remote.test");
        assert_eq!(meta.mail_from, "a@local.test");
        assert_eq!(loaded, body);

        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            std::fs::metadata(store.body_path(&ids[0])).unwrap().nlink(),
            1
        );
    }

    #[tokio::test]
    async fn write_shared_body_bytes_identical_across_links() {
        let dir = tempfile::tempdir().unwrap();
        let store = QueueStore::new(dir.path().to_path_buf());
        store.ensure_dir().await.unwrap();

        let body = b"From: a@local.test\r\n\r\nshared payload v1";
        let rcpts = vec![
            "a@remote.test".into(),
            "b@remote.test".into(),
            "c@remote.test".into(),
        ];
        let ids = store
            .write_shared("a@local.test", &rcpts, body, now_unix())
            .await
            .unwrap();

        for id in &ids {
            let (_, loaded) = store.load(id).await.unwrap();
            assert_eq!(loaded, body);
        }
    }

    /// Removing one delivered recipient drops a single hard link; remaining
    /// recipients still load the shared body (refcount-by-hard-link).
    #[tokio::test]
    async fn write_shared_remove_drops_one_link_keeps_others() {
        let dir = tempfile::tempdir().unwrap();
        let store = QueueStore::new(dir.path().to_path_buf());
        store.ensure_dir().await.unwrap();

        let body = b"shared";
        let ids = store
            .write_shared(
                "a@local.test",
                &[
                    "u0@remote.test".into(),
                    "u1@remote.test".into(),
                    "u2@remote.test".into(),
                ],
                body,
                now_unix(),
            )
            .await
            .unwrap();

        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            std::fs::metadata(store.body_path(&ids[0])).unwrap().nlink() as usize,
            3
        );

        store.remove(&ids[0]).await;
        assert_eq!(store.count_entries().await.unwrap(), 2);
        assert!(
            !store.body_path(&ids[0]).exists(),
            "removed entry's body path must be gone"
        );

        let nlink = std::fs::metadata(store.body_path(&ids[1]))
            .unwrap()
            .nlink() as usize;
        assert_eq!(nlink, 2, "remaining hard links must survive remove");

        for id in &ids[1..] {
            let (_, loaded) = store.load(id).await.unwrap();
            assert_eq!(loaded, body);
        }

        store.remove(&ids[1]).await;
        store.remove(&ids[2]).await;
        assert_eq!(store.count_entries().await.unwrap(), 0);
        assert!(list_queue_files(dir.path()).is_empty());
    }

    /// After a rolled-back failure, a later successful write must still work
    /// (failpoint disarms; no sticky error state).
    #[tokio::test]
    async fn write_shared_succeeds_after_prior_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let store = QueueStore::new(dir.path().to_path_buf());
        store.ensure_dir().await.unwrap();

        {
            let _fp = test_failpoints::arm(test_failpoints::AFTER_LINKS);
            let err = store
                .write_shared(
                    "a@local.test",
                    &["u0@remote.test".into(), "u1@remote.test".into()],
                    b"fail-me",
                    now_unix(),
                )
                .await
                .unwrap_err();
            assert!(err.to_string().contains("test failpoint"));
        }
        assert!(list_queue_files(dir.path()).is_empty());

        let ids = store
            .write_shared(
                "a@local.test",
                &["u0@remote.test".into(), "u1@remote.test".into()],
                b"ok-me",
                now_unix(),
            )
            .await
            .unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(store.count_entries().await.unwrap(), 2);
        let (_, loaded) = store.load(&ids[0]).await.unwrap();
        assert_eq!(loaded, b"ok-me");
    }
}
