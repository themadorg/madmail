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
    pub async fn write_shared(
        &self,
        mail_from: &str,
        rcpts: &[String],
        body: &[u8],
        next_attempt_unix: u64,
    ) -> Result<Vec<String>> {
        let now = now_unix();
        let ids: Vec<String> = rcpts
            .iter()
            .map(|_| uuid::Uuid::new_v4().to_string())
            .collect();

        // 1. Write the body exactly once (the only real copy on disk).
        self.write_body(&ids[0], body).await?;
        let canonical = self.body_path(&ids[0]);

        // 2. Hard-link that single body into every other recipient's entry.
        for id in &ids[1..] {
            fs::hard_link(&canonical, self.body_path(id)).await?;
        }

        // 3. Write per-recipient meta. Written after the body/links so an entry is
        //    never observable (by reload or the worker) without its body present.
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
        }

        Ok(ids)
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
