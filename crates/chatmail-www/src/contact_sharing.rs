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

//! Delta Chat contact sharing HTTP (`/share`, `/{slug}`).

use std::path::Path;
use std::sync::Arc;

use chatmail_db::init_sharing_db;
use chatmail_types::Result;
use sqlx::SqlitePool;
use tokio::sync::OnceCell;

/// Slugs that must not be used for contact pages (Madmail Go reserved list).
pub fn is_reserved_slug(slug: &str) -> bool {
    matches!(
        slug,
        "share"
            | "qr"
            | "new"
            | "madmail"
            | "mxdeliv"
            | "main.css"
            | "index.html"
            | "info.html"
            | "security.html"
            | "deploy.html"
            | "app"
            | "docs"
            | "webimap"
            | "websmtp"
            | "inv"
    )
}

pub struct SharingStore {
    pool: OnceCell<SqlitePool>,
    db_path: std::path::PathBuf,
}

impl SharingStore {
    pub fn new(_state_dir: &Path, db_path: std::path::PathBuf) -> Arc<Self> {
        Arc::new(Self {
            pool: OnceCell::new(),
            db_path,
        })
    }

    pub async fn pool(&self) -> Result<&SqlitePool> {
        self.pool
            .get_or_try_init(|| async { init_sharing_db(&self.db_path).await })
            .await
    }
}
