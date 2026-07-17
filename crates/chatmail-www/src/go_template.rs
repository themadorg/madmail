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

//! Translate Madmail Go `html/template` syntax to Minijinja at render time.

/// Accept Go-style (`{{if .Field}}`) or existing Minijinja templates.
pub fn prepare_template(input: &str) -> String {
    post_process(&go_to_minijinja(input))
}

fn go_to_minijinja(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '{' {
            if let Some(&(_, '{')) = chars.peek() {
                if let Some((converted, new_i)) = convert_action(input, i) {
                    out.push_str(&converted);
                    while let Some(&(idx, _)) = chars.peek() {
                        if idx < new_i {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    continue;
                }
            }
        }
        out.push(c);
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
            return Some((format!(r#"{{% if {field} == "{value}" %}}"#), new_i));
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
        let o = prepare_template(s);
        assert!(o.contains("{% if RegistrationOpen %}"));
        assert!(o.contains("{{ MailDomain | clean_domain }}"));
    }

    #[test]
    fn leaves_minijinja_unchanged() {
        let s = r#"{% if RegistrationOpen %}{{ MailDomain }}{% endif %}"#;
        assert_eq!(prepare_template(s), s);
    }
}
