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
    parse_cli_with_config(state_dir, &config, subcommand)
}

/// Minimal `maddy.conf` with Shadowsocks configured in the `chatmail` block.
pub fn write_ss_test_config(dir: &Path) -> PathBuf {
    let config = dir.join("test_ss.conf");
    std::fs::write(
        &config,
        r#"
chatmail tcp://0.0.0.0:80 {
    mail_domain test.example
    ss_addr 0.0.0.0:8388
    ss_password config-pass
    ss_cipher aes-128-gcm
}
"#,
    )
    .expect("write ss test config");
    config
}

/// Minimal `maddy.conf` with Iroh discovery on the `imap` block.
pub fn write_iroh_test_config(dir: &Path) -> PathBuf {
    let config = dir.join("test_iroh.conf");
    std::fs::write(
        &config,
        r#"
hostname mail.test
public_ip 203.0.113.50
imap tcp://0.0.0.0:143 {
    iroh_relay_url http://203.0.113.50:3340
}
"#,
    )
    .expect("write iroh test config");
    config
}

/// Minimal `maddy.conf` with a DNS `primary_domain` (for `dkim show`).
pub fn write_dkim_test_config(dir: &Path) -> PathBuf {
    let config = dir.join("test_dkim.conf");
    std::fs::write(
        &config,
        r#"
hostname mail.example.org
$(primary_domain) = example.org
"#,
    )
    .expect("write dkim test config");
    config
}

/// Minimal `maddy.conf` without Iroh (for `iroh install`).
pub fn write_plain_imap_test_config(dir: &Path) -> PathBuf {
    let config = dir.join("test_plain_imap.conf");
    std::fs::write(
        &config,
        r#"
hostname mail.test
public_ip 203.0.113.50
imap tcp://0.0.0.0:143 {
    auth &local_authdb
}
"#,
    )
    .expect("write plain imap test config");
    config
}

pub fn parse_cli_with_config(state_dir: &Path, config: &Path, subcommand: &[&str]) -> Cli {
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
