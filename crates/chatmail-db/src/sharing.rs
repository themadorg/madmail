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

//! Delta Chat contact sharing (`sharing.db` / `contacts` table, Madmail `mdb.Contact`).

use std::path::Path;

use chatmail_types::{ChatmailError, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SharingContact {
    pub slug: String,
    pub url: String,
    pub name: String,
    pub created_at: String,
}

const CONTACTS_DDL: &str = r"
CREATE TABLE IF NOT EXISTS contacts (
    slug TEXT PRIMARY KEY NOT NULL,
    url TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
";

/// Open or create the sharing SQLite database (default: `{state_dir}/sharing.db`).
pub async fn init_sharing_db(path: &Path) -> Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", path.display()))?
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await?;
    sqlx::query(CONTACTS_DDL).execute(&pool).await?;
    Ok(pool)
}

pub fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        return Err(ChatmailError::config("SLUG is required"));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric())
    {
        return Err(ChatmailError::config(
            "SLUG must be alphanumeric (a-z, A-Z, 0-9)",
        ));
    }
    Ok(())
}

/// Convert Delta Chat web invite to `openpgp4fpr:` (Madmail `sharingCreateInternal`).
pub fn normalize_sharing_url(raw: &str) -> Result<String> {
    let raw = raw.trim();
    if raw == "reserved" || raw.starts_with("openpgp4fpr:") {
        return Ok(raw.to_string());
    }
    const PREFIX: &str = "https://i.delta.chat/#";
    if let Some(content) = raw.strip_prefix(PREFIX) {
        if let Some(idx) = content.find('&') {
            return Ok(format!(
                "openpgp4fpr:{}#{}",
                &content[..idx],
                &content[idx + 1..]
            ));
        }
        return Ok(format!("openpgp4fpr:{content}"));
    }
    Err(ChatmailError::config(
        "URL must be DeltaChat web link (https://i.delta.chat/#...) or openpgp4fpr: link",
    ))
}

pub async fn list_sharing_contacts(pool: &SqlitePool) -> Result<Vec<SharingContact>> {
    let rows = sqlx::query_as::<_, SharingContact>(
        "SELECT slug, url, name, created_at FROM contacts ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn create_sharing_contact(
    pool: &SqlitePool,
    slug: &str,
    raw_url: &str,
    name: &str,
) -> Result<()> {
    validate_slug(slug)?;
    let url = normalize_sharing_url(raw_url)?;
    sqlx::query("INSERT INTO contacts (slug, url, name) VALUES (?, ?, ?)")
        .bind(slug)
        .bind(&url)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_sharing_contact(pool: &SqlitePool, slug: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM contacts WHERE slug = ?")
        .bind(slug)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn update_sharing_contact(
    pool: &SqlitePool,
    slug: &str,
    raw_url: &str,
    name: Option<&str>,
) -> Result<bool> {
    let url = normalize_sharing_url(raw_url)?;
    let result = if let Some(n) = name {
        sqlx::query("UPDATE contacts SET url = ?, name = ? WHERE slug = ?")
            .bind(&url)
            .bind(n)
            .bind(slug)
            .execute(pool)
            .await?
    } else {
        sqlx::query("UPDATE contacts SET url = ? WHERE slug = ?")
            .bind(&url)
            .bind(slug)
            .execute(pool)
            .await?
    };
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_delta_web_link() {
        let u = normalize_sharing_url("https://i.delta.chat/#ABCDEF&x=1").unwrap();
        assert_eq!(u, "openpgp4fpr:ABCDEF#x=1");
    }

    #[tokio::test]
    async fn sharing_crud_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sharing.db");
        let pool = init_sharing_db(&path).await.unwrap();
        create_sharing_contact(
            &pool,
            "alice",
            "https://i.delta.chat/#fp",
            "Alice",
        )
        .await
        .unwrap();
        let rows = list_sharing_contacts(&pool).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].slug, "alice");
        assert!(remove_sharing_contact(&pool, "alice").await.unwrap());
    }
}
