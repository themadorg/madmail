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

const LOCAL_STATE: &str = "./data";
const LOCAL_CONFIG: &str = "./data/chatmail.toml";
const PROD_STATE: &str = "/var/lib/madmail";
const PROD_CONFIG: &str = "/etc/madmail/madmail.conf";
const LEGACY_PROD_STATE: &str = "/var/lib/maddy";
const LEGACY_PROD_CONFIG: &str = "/etc/maddy/maddy.conf";

/// True when `path` looks like a local dev state dir (repo `./data` or prior server run).
pub fn is_local_dev_state_dir(path: &Path) -> bool {
    path.join("admin_token").is_file()
        || path.join(crate::db_path::CHATMAIL_RS_DB).is_file()
        || path.join(crate::db_path::MADMAIL_CREDENTIALS_DB).is_file()
        || path.join("chatmail.toml").is_file()
}

/// Default `--state-dir`: `./data` when present, else `/var/lib/maddy` if usable, else `./data`.
pub fn detect_default_state_dir() -> PathBuf {
    if let Ok(s) = std::env::var("CHATMAIL_STATE_DIR") {
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }

    let local = PathBuf::from(LOCAL_STATE);
    if is_local_dev_state_dir(&local) {
        return local;
    }

    for candidate in [PROD_STATE, LEGACY_PROD_STATE] {
        let prod = PathBuf::from(candidate);
        if state_dir_usable(&prod) {
            return prod;
        }
    }

    local
}

/// Default `--config`: local `chatmail.toml` when present, else production `maddy.conf`.
pub fn detect_default_config_path() -> PathBuf {
    let local = PathBuf::from(LOCAL_CONFIG);
    if local.is_file() {
        return local;
    }
    for candidate in [PROD_CONFIG, LEGACY_PROD_CONFIG] {
        let prod = PathBuf::from(candidate);
        if prod.is_file() {
            return prod;
        }
    }
    local
}

fn state_dir_usable(path: &Path) -> bool {
    if is_local_dev_state_dir(path) {
        return true;
    }
    path.is_dir() && std::fs::read_dir(path).is_ok()
}

fn argv_has_flag(flag: &str) -> bool {
    std::env::args().any(|a| a == flag || a.starts_with(&format!("{flag}=")))
}

/// Apply auto-detected `./data` paths when the user did not pass `--config` / `--state-dir` (or env).
pub fn apply_cli_defaults(args: &mut crate::cli::Args) {
    if !argv_has_flag("--state-dir")
        && !argv_has_flag("--libexec")
        && std::env::var("CHATMAIL_STATE_DIR").is_err()
    {
        args.state_dir = detect_default_state_dir();
    }
    if !argv_has_flag("--config") && std::env::var("CHATMAIL_CONFIG").is_err() {
        args.config = detect_default_config_path();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_local_data_when_markers_exist() {
        if Path::new(LOCAL_STATE).join("chatmail.toml").is_file()
            || Path::new(LOCAL_STATE).join("admin_token").is_file()
        {
            assert_eq!(detect_default_state_dir(), PathBuf::from(LOCAL_STATE));
        }
    }
}
