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

use std::collections::HashMap;
use std::path::PathBuf;

use chatmail_config::{
    effective_app_db_path, effective_database_config, load_config, resolve_state_dir, AppConfig,
    Args, DatabaseConfig,
};
use chatmail_db::{init_db_from_config, list_double_underscore_settings, DbPool};
use chatmail_types::{ChatmailError, Result};

/// Resolved CLI environment: config, state dir, and DB connection.
pub struct CtlContext {
    pub config: AppConfig,
    pub state_dir: PathBuf,
    pub database: DatabaseConfig,
    /// SQLite file path when using `sqlite3` (display / legacy checks).
    pub db_path: PathBuf,
}

impl CtlContext {
    pub fn from_args(args: &Args) -> Result<Self> {
        let config = if args.config.is_file() {
            load_config(&args.config)?
        } else {
            AppConfig::default()
        };
        let state_dir = resolve_state_dir(args.state_dir.clone(), &config);
        let database = effective_database_config(&state_dir, &config);
        let db_path = effective_app_db_path(&state_dir, &config);
        Ok(Self {
            config,
            state_dir,
            database,
            db_path,
        })
    }

    pub fn require_db(&self) -> Result<()> {
        if self.database.is_postgres() {
            return Ok(());
        }
        if self.db_path.is_file() {
            Ok(())
        } else {
            Err(ChatmailError::config(format!(
                "no database at {} — start the server first:\n  \
                 cargo run -p chatmail -- --state-dir {}",
                self.db_path.display(),
                self.state_dir.display()
            )))
        }
    }

    /// Open the application DB, creating it and running migrations when missing.
    pub async fn open_pool(&self) -> Result<DbPool> {
        init_db_from_config(&self.database).await
    }

    pub async fn load_settings_map(&self) -> Result<HashMap<String, String>> {
        let pool = self.open_pool().await?;
        let rows = list_double_underscore_settings(&pool).await?;
        Ok(rows.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_config::Args;
    use clap::Parser;

    #[tokio::test]
    async fn open_pool_creates_database_on_fresh_state_dir() {
        let dir = tempfile::tempdir().unwrap();
        let args = Args::try_parse_from([
            "chatmail",
            "--state-dir",
            dir.path().to_str().unwrap(),
            "--config",
            dir.path().join("missing.conf").to_str().unwrap(),
        ])
        .unwrap();
        let ctx = CtlContext::from_args(&args).unwrap();
        assert!(!ctx.db_path.is_file());
        let pool = ctx.open_pool().await.unwrap();
        assert!(ctx.db_path.is_file());
        drop(pool);
    }
}
