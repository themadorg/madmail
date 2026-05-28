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

//! Update `www_dir` in `maddy.conf` / `chatmail.toml` (Madmail `html-serve`).

use std::path::Path;

use chatmail_types::{ChatmailError, Result};

/// Set or clear the chatmail `www_dir` directive in the config file on disk.
pub fn update_config_www_dir(config_path: &Path, www_dir: Option<&Path>) -> Result<()> {
    if !config_path.is_file() {
        return Err(ChatmailError::config(format!(
            "config file not found: {}",
            config_path.display()
        )));
    }

    let ext = config_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    if ext == "toml" {
        update_toml_www_dir(config_path, www_dir)
    } else {
        update_maddy_www_dir(config_path, www_dir)
    }
}

fn update_toml_www_dir(config_path: &Path, www_dir: Option<&Path>) -> Result<()> {
    let raw = std::fs::read_to_string(config_path)?;
    let mut doc: toml::Table = toml::from_str(&raw)
        .map_err(|e| ChatmailError::config(format!("invalid TOML: {e}")))?;

    match www_dir {
        Some(p) => {
            let abs = p
                .canonicalize()
                .unwrap_or_else(|_| p.to_path_buf());
            doc.insert(
                "www_dir".into(),
                toml::Value::String(abs.display().to_string()),
            );
        }
        None => {
            doc.remove("www_dir");
        }
    }

    let out = toml::to_string_pretty(&doc)
        .map_err(|e| ChatmailError::config(format!("serialize TOML: {e}")))?;
    std::fs::write(config_path, out).map_err(ChatmailError::from)?;
    Ok(())
}

fn update_maddy_www_dir(config_path: &Path, www_dir: Option<&Path>) -> Result<()> {
    let data = std::fs::read_to_string(config_path)?;
    let lines: Vec<&str> = data.lines().collect();
    let mut new_lines: Vec<String> = Vec::new();
    let mut in_chatmail = false;
    let mut updated = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("chatmail ") && trimmed.ends_with('{') {
            in_chatmail = true;
            new_lines.push(line.to_string());
            continue;
        }

        if in_chatmail && trimmed == "}" {
            if let Some(dir) = www_dir {
                let abs = dir
                    .canonicalize()
                    .unwrap_or_else(|_| dir.to_path_buf());
                new_lines.push(format!("    www_dir {}", abs.display()));
            }
            updated = true;
            in_chatmail = false;
            new_lines.push(line.to_string());
            continue;
        }

        if in_chatmail && trimmed.starts_with("www_dir ") {
            continue;
        }

        new_lines.push(line.to_string());
    }

    if !updated {
        return Err(ChatmailError::config(
            "no chatmail { ... } block found in config (cannot set www_dir)",
        ));
    }

    std::fs::write(config_path, new_lines.join("\n") + "\n").map_err(ChatmailError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maddy_conf_www_dir_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("maddy.conf");
        std::fs::write(
            &path,
            "chatmail tcp://0.0.0.0:80 {\n    mail_domain example.org\n}\n",
        )
        .unwrap();
        let www = dir.path().join("custom-www");
        std::fs::create_dir_all(&www).unwrap();
        update_config_www_dir(&path, Some(&www)).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("www_dir"));
        update_config_www_dir(&path, None).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(!body.contains("www_dir"));
    }
}
