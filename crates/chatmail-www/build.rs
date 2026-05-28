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

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest_dir.join("www-src");
    let dst = manifest_dir.join("www");

    println!("cargo:rerun-if-changed=www-src");
    if !src.is_dir() {
        panic!(
            "www-src missing; copy from context/madmail/internal/endpoint/chatmail/www"
        );
    }

    if dst.exists() {
        fs::remove_dir_all(&dst).expect("remove old www");
    }
    copy_dir_all(&src, &dst).expect("copy www-src");

    for entry in walkdir(&dst) {
        if entry.extension().and_then(|e| e.to_str()) == Some("html") {
            let raw = fs::read_to_string(&entry).expect("read html");
            let converted = post_process(&go_to_minijinja(&raw));
            fs::write(&entry, converted).expect("write converted html");
        }
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

fn walkdir(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(rd) = fs::read_dir(&path) else {
            continue;
        };
        for ent in rd.flatten() {
            let p = ent.path();
            if ent.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// Convert Go `html/template` syntax used by Madmail www to Minijinja.
fn go_to_minijinja(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some((converted, new_i)) = convert_action(input, i) {
                out.push_str(&converted);
                i = new_i;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn convert_action(s: &str, start: usize) -> Option<(String, usize)> {
    let rest = &s[start + 2..];
    let end = rest.find("}}")?;
    let inner = rest[..end].trim();
    let new_i = start + 2 + end + 2;

    if inner == "else" {
        return Some(("{% else %}".into(), new_i));
    }
    if inner == "end" {
        return Some(("{% endif %}".into(), new_i));
    }
    if let Some(stripped) = inner.strip_prefix("if eq .") {
        if let Some((field, value)) = stripped.split_once(' ') {
            let value = value.trim().trim_matches('"');
            return Some((
                format!(r#"{{% if {field} == "{value}" %}}"#),
                new_i,
            ));
        }
    }
    if let Some(field) = inner.strip_prefix("if .") {
        return Some((format!("{{% if {field} %}}"), new_i));
    }
    if let Some(expr) = inner.strip_prefix('.') {
        if let Some((field, filter)) = expr.split_once(" | ") {
            let filter = match filter.trim() {
                "cleanDomain" => "clean_domain",
                other => other,
            };
            return Some((format!("{{{{ {field} | {filter} }}}}"), new_i));
        }
        return Some((format!("{{{{ {expr} }}}}"), new_i));
    }
    None
}

fn post_process(s: &str) -> String {
    let mut s = s.to_string();
    s = s.replace(
        "{{slice .Custom.Name 0 1 | printf \"%s\" | upper}}",
        "{{ Custom.Name[0:1] | upper }}",
    );
    s = s.replace("| safeURL", "");
    s = s.replace("| formatBytes", "| format_bytes");
    s = s.replace("| safeHTML", "| safe_html");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_if_and_field() {
        let s = r#"{{if .RegistrationOpen}}yes{{else}}no{{end}} {{.MailDomain | cleanDomain}}"#;
        let o = go_to_minijinja(s);
        assert!(o.contains("{% if RegistrationOpen %}"));
        assert!(o.contains("{{ MailDomain | clean_domain }}"));
    }
}
