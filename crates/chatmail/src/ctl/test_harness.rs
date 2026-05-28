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

//! Shared temp DB + CLI argv for ctl unit/integration tests.

use std::path::{Path, PathBuf};

use chatmail_config::{effective_app_db_path, AppConfig, Args, Cli};
use chatmail_db::{init_db, DbPool};
use clap::Parser;
use tempfile::TempDir;

/// Temp state dir with migrated SQLite DB and CLI `Args`.
pub async fn setup_ctl_env() -> (TempDir, Args, PathBuf, DbPool) {
    let dir = TempDir::new().expect("tempdir");
    let db_path = effective_app_db_path(dir.path(), &AppConfig::default());
    let pool = init_db(&db_path).await.expect("init_db");
    let args = ctl_args(dir.path());
    (dir, args, db_path, pool)
}

pub fn ctl_args(state_dir: &Path) -> Args {
    let config = state_dir.join("_test_no_config_.conf");
    Cli::try_parse_from([
        "chatmail",
        "--state-dir",
        state_dir.to_str().expect("utf8 state_dir"),
        "--config",
        config.to_str().expect("utf8 config"),
    ])
    .expect("parse ctl test argv")
    .args
}

pub fn parse_cli(state_dir: &Path, subcommand: &[&str]) -> Cli {
    let config = state_dir.join("_test_no_config_.conf");
    let mut argv = vec![
        "chatmail",
        "--state-dir",
        state_dir.to_str().expect("utf8 state_dir"),
        "--config",
        config.to_str().expect("utf8 config"),
    ];
    argv.extend_from_slice(subcommand);
    Cli::try_parse_from(argv).expect("parse ctl subcommand")
}
