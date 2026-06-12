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

//! Application database connection (Madmail `auth.pass_table` / `table sql_table`).

use std::path::{Path, PathBuf};

use crate::{resolve_state_path, AppConfig};

/// Madmail default: `auth.pass_table` → `credentials.db` in the state directory.
pub const MADMAIL_CREDENTIALS_DB: &str = "credentials.db";

/// madmail-v2 dev default when not using Madmail layout.
pub const CHATMAIL_RS_DB: &str = "chatmail.db";

/// Supported `driver` values in `maddy.conf` (`sqlite3`, `postgres`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbDriver {
    Sqlite3,
    Postgres,
}

impl DbDriver {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("postgres" | "postgresql") => Self::Postgres,
            _ => Self::Sqlite3,
        }
    }

    pub fn is_postgres(self) -> bool {
        matches!(self, Self::Postgres)
    }
}

/// Resolved credentials DB connection (Madmail `table sql_table { driver; dsn }`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseConfig {
    pub driver: DbDriver,
    /// SQLite: absolute path to the DB file. Postgres: libpq-style connection string.
    pub dsn: String,
}

impl DatabaseConfig {
    pub fn is_postgres(&self) -> bool {
        self.driver.is_postgres()
    }

    /// Human-readable location for logs / CLI errors.
    pub fn display_location(&self) -> String {
        self.dsn.clone()
    }
}

/// Resolve credentials DB driver + DSN from `maddy.conf` (`auth.pass_table` / `sql_table`).
///
/// Priority:
/// 1. `credentials_driver` + `credentials_dsn` from config
/// 2. Existing `credentials.db` (Madmail install)
/// 3. Existing `chatmail.db` (madmail-v2-only dev)
/// 4. New installs: `credentials.db` (Madmail-compatible default)
pub fn effective_database_config(state_dir: &Path, config: &AppConfig) -> DatabaseConfig {
    let driver = DbDriver::parse(config.credentials_driver.as_deref());
    if let Some(dsn) = config.credentials_dsn.as_deref().filter(|s| !s.is_empty()) {
        return DatabaseConfig {
            driver,
            dsn: resolve_credentials_dsn(state_dir, driver, dsn),
        };
    }
    let cred = state_dir.join(MADMAIL_CREDENTIALS_DB);
    let chat = state_dir.join(CHATMAIL_RS_DB);
    let path = if cred.is_file() {
        cred
    } else if chat.is_file() {
        chat
    } else {
        cred
    };
    DatabaseConfig {
        driver: DbDriver::Sqlite3,
        dsn: path.display().to_string(),
    }
}

fn resolve_credentials_dsn(state_dir: &Path, driver: DbDriver, dsn: &str) -> String {
    if driver.is_postgres() {
        dsn.to_string()
    } else {
        resolve_state_path(state_dir, dsn).display().to_string()
    }
}

/// SQLite file path used for accounts, settings, etc. (legacy helper).
///
/// Prefer [`effective_database_config`] when opening the DB.
pub fn effective_app_db_path(state_dir: &Path, config: &AppConfig) -> PathBuf {
    let db = effective_database_config(state_dir, config);
    PathBuf::from(db.dsn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_credentials_db_when_present() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MADMAIL_CREDENTIALS_DB), b"x").unwrap();
        let db = effective_database_config(dir.path(), &AppConfig::default());
        assert_eq!(db.driver, DbDriver::Sqlite3);
        assert_eq!(
            db.dsn,
            dir.path()
                .join(MADMAIL_CREDENTIALS_DB)
                .display()
                .to_string()
        );
    }

    #[test]
    fn postgres_dsn_is_not_resolved_against_state_dir() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = AppConfig {
            credentials_driver: Some("postgres".into()),
            credentials_dsn: Some(
                "host=127.0.0.1 port=5432 user=maddy dbname=maddy sslmode=disable".into(),
            ),
            ..Default::default()
        };
        let db = effective_database_config(dir.path(), &cfg);
        assert_eq!(db.driver, DbDriver::Postgres);
        assert_eq!(
            db.dsn,
            "host=127.0.0.1 port=5432 user=maddy dbname=maddy sslmode=disable"
        );
    }
}
