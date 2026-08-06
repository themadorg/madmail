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

//! Persist Iroh relay settings in `maddy.conf` / `chatmail.toml`
//! (`madmail install --enable-iroh` parity for existing deployments).

use std::path::Path;

use chatmail_types::{ChatmailError, Result};

/// Ensure the on-disk config enables Iroh discovery.
///
/// Madmail install injects `iroh_relay_url http://$(public_ip):3340` into an `imap`
/// block. This helper does the same without a full reinstall.
///
/// When `iroh_relay_url` is already present, the file is left unchanged.
pub fn update_config_iroh_enable(config_path: &Path, relay_url: &str) -> Result<()> {
    let relay_url = relay_url.trim();
    if relay_url.is_empty() {
        return Err(ChatmailError::config(
            "iroh relay URL must not be empty (expected http://host:3340)",
        ));
    }
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
        update_toml_iroh(config_path, relay_url)
    } else {
        update_maddy_iroh(config_path, relay_url)
    }
}

fn update_toml_iroh(config_path: &Path, relay_url: &str) -> Result<()> {
    let raw = std::fs::read_to_string(config_path)?;
    let mut doc: toml::Table =
        toml::from_str(&raw).map_err(|e| ChatmailError::config(format!("invalid TOML: {e}")))?;

    doc.insert("iroh_enable".into(), toml::Value::Boolean(true));
    // Keep an existing non-empty URL; otherwise set the provided default.
    let keep_url = doc
        .get("iroh_relay_url")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.trim().is_empty());
    if !keep_url {
        doc.insert(
            "iroh_relay_url".into(),
            toml::Value::String(relay_url.to_string()),
        );
    }

    let out = toml::to_string_pretty(&doc)
        .map_err(|e| ChatmailError::config(format!("serialize TOML: {e}")))?;
    std::fs::write(config_path, out).map_err(ChatmailError::from)?;
    Ok(())
}

fn update_maddy_iroh(config_path: &Path, relay_url: &str) -> Result<()> {
    let data = std::fs::read_to_string(config_path)?;
    if data.lines().any(|l| {
        let t = l.trim();
        t.starts_with("iroh_relay_url ") && !t.starts_with('#')
    }) {
        return Ok(());
    }

    let lines: Vec<&str> = data.lines().collect();
    let mut new_lines: Vec<String> = Vec::with_capacity(lines.len() + 2);
    let mut inserted = false;
    let mut depth: i32 = 0;
    let mut in_imap = false;

    for line in lines {
        let trimmed = line.trim();
        let opens = trimmed.matches('{').count() as i32;
        let closes = trimmed.matches('}').count() as i32;

        if !inserted && depth == 0 && trimmed.starts_with("imap ") {
            in_imap = true;
        }

        if in_imap
            && !inserted
            && closes > 0
            && depth + opens - closes <= 0
            && (trimmed == "}" || trimmed.starts_with('}'))
        {
            new_lines.push(format!("    iroh_relay_url {relay_url}"));
            inserted = true;
            in_imap = false;
        }

        new_lines.push(line.to_string());
        depth += opens - closes;
        if depth < 0 {
            depth = 0;
        }
        if in_imap && depth == 0 {
            in_imap = false;
        }
    }

    if !inserted {
        new_lines.push(String::new());
        new_lines.push("# Added by madmail iroh install".into());
        new_lines.push("imap tcp://0.0.0.0:143 {".into());
        new_lines.push(format!("    iroh_relay_url {relay_url}"));
        new_lines.push("}".into());
    }

    std::fs::write(config_path, new_lines.join("\n") + "\n").map_err(ChatmailError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn maddy_inserts_iroh_into_imap_block() {
        let mut f = NamedTempFile::new().unwrap();
        write!(
            f,
            r#"hostname mail.example.org
imap tcp://0.0.0.0:143 {{
    auth &local_authdb
}}
"#
        )
        .unwrap();
        update_config_iroh_enable(f.path(), "http://203.0.113.10:3340").unwrap();
        let out = std::fs::read_to_string(f.path()).unwrap();
        assert!(out.contains("iroh_relay_url http://203.0.113.10:3340"));
        assert!(out.find("iroh_relay_url").unwrap() < out.rfind('}').unwrap());
    }

    #[test]
    fn maddy_idempotent_when_already_present() {
        let mut f = NamedTempFile::new().unwrap();
        write!(
            f,
            r#"imap tcp://0.0.0.0:143 {{
    iroh_relay_url http://old.example:3340
}}
"#
        )
        .unwrap();
        update_config_iroh_enable(f.path(), "http://new.example:3340").unwrap();
        let out = std::fs::read_to_string(f.path()).unwrap();
        assert!(out.contains("http://old.example:3340"));
        assert!(!out.contains("http://new.example:3340"));
    }

    #[test]
    fn toml_sets_enable_and_url() {
        let mut f = NamedTempFile::with_suffix(".toml").unwrap();
        writeln!(f, "hostname = \"mail.example.org\"").unwrap();
        update_config_iroh_enable(f.path(), "http://203.0.113.10:3340").unwrap();
        let out = std::fs::read_to_string(f.path()).unwrap();
        assert!(out.contains("iroh_enable = true"));
        assert!(out.contains("203.0.113.10"));
    }
}
