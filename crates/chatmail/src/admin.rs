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

use std::path::Path;

use chatmail_types::Result;

pub const ADMIN_TOKEN_FILE: &str = "admin_token";
const TOKEN_HEX_LEN: usize = 64;

/// Resolve admin token: config `admin_token` directive, else `{state_dir}/admin_token` file.
pub fn resolve_admin_token(state_dir: &Path, config: &chatmail_config::AppConfig) -> Result<String> {
    if let Some(ref token) = config.admin_token {
        match token.as_str() {
            "disabled" => {
                return Err(chatmail_types::ChatmailError::config(
                    "admin API is disabled in config (admin_token disabled)",
                ));
            }
            t if !t.is_empty() => return Ok(t.to_string()),
            _ => {}
        }
    }
    ensure_admin_token(state_dir)
}

/// Load or create the admin API bearer token at `{state_dir}/admin_token`.
pub fn ensure_admin_token(state_dir: &Path) -> Result<String> {
    let token_path = state_dir.join(ADMIN_TOKEN_FILE);
    if let Ok(data) = std::fs::read_to_string(&token_path) {
        let token = data.trim().to_string();
        if token.len() == TOKEN_HEX_LEN && token.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(token);
        }
    }

    let token = generate_token_hex()?;
    write_admin_token(&token_path, &token)?;
    Ok(token)
}

fn generate_token_hex() -> Result<String> {
    let mut bytes = [0u8; TOKEN_HEX_LEN / 2];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn write_admin_token(path: &Path, token: &str) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);

    let mut file = opts.open(path)?;
    writeln!(file, "{token}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P1-UT07: token is created once and preserved across calls.
    #[test]
    fn p1_ut07_admin_token_generation() {
        let dir = tempfile::tempdir().unwrap();
        let first = ensure_admin_token(dir.path()).unwrap();
        assert_eq!(first.len(), TOKEN_HEX_LEN);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(ADMIN_TOKEN_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        let second = ensure_admin_token(dir.path()).unwrap();
        assert_eq!(first, second, "must not regenerate existing token");

        let third = std::fs::read_to_string(dir.path().join(ADMIN_TOKEN_FILE)).unwrap();
        assert_eq!(third.trim(), first);
    }
}
