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
//!
//! Custom `www_dir` pages exported from Go Madmail often keep forms like
//! `{{if not .RegistrationOpen}}` / `{{.MailDomain | cleanDomain}}`. Without
//! conversion Minijinja fails with:
//! `syntax error: unexpected '.', expected in`.

/// Accept Go-style (`{{if .Field}}`, `{{if not .Field}}`) or existing Minijinja templates.
pub fn prepare_template(input: &str) -> String {
    post_process(&go_to_minijinja(input))
}

/// True when `src` still uses Madmail Go `html/template` markers (not native Minijinja).
///
/// Used by `html-migrate` / upgrade to decide whether on-disk custom `www_dir` HTML
/// should be rewritten. Pure Minijinja (`{% if %}`, `{{ Field }}` without a leading `.`)
/// returns false.
pub fn looks_like_go_template(src: &str) -> bool {
    if has_go_control_or_field_action(src) {
        return true;
    }
    // Go filter names (Minijinja uses snake_case)
    if src.contains("| cleanDomain")
        || src.contains("|cleanDomain")
        || src.contains("| safeHTML")
        || src.contains("|safeHTML")
        || src.contains("| formatBytes")
        || src.contains("|formatBytes")
        || src.contains("| safeURL")
        || src.contains("|safeURL")
    {
        return true;
    }
    false
}

/// Scan `{{ … }}` actions for Go control flow / dotted field forms.
fn has_go_control_or_field_action(src: &str) -> bool {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            let rest = &src[i + 2..];
            if let Some(end) = rest.find("}}") {
                let norm = normalize_action(rest[..end].trim());
                if is_go_action(&norm) {
                    return true;
                }
                i += 2 + end + 2;
                continue;
            }
        }
        i += 1;
    }
    false
}

fn is_go_action(norm: &str) -> bool {
    matches!(norm, "else" | "end")
        || norm.starts_with("if .")
        || norm.starts_with("if not .")
        || norm.starts_with("if eq .")
        || norm.starts_with('.')
}

/// Collapse runs of whitespace so `if  not  .X` matches `if not .X`.
fn normalize_action(inner: &str) -> String {
    inner.split_whitespace().collect::<Vec<_>>().join(" ")
}

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
    let inner_raw = rest[..end].trim();
    let new_i = start + 2 + end + 2;
    let inner = normalize_action(inner_raw);

    if inner == "else" {
        return Some(("{% else %}".into(), new_i));
    }
    if inner == "end" {
        return Some(("{% endif %}".into(), new_i));
    }
    if let Some(stripped) = inner.strip_prefix("if eq .") {
        // `if eq .Language "fa"` or `if eq .Language fa`
        let stripped = stripped.trim();
        if let Some((field, value)) = stripped.split_once(' ') {
            let value = value.trim().trim_matches('"');
            return Some((format!(r#"{{% if {field} == "{value}" %}}"#), new_i));
        }
    }
    // Go: `{{if not .RegistrationOpen}}` → Minijinja `{% if not RegistrationOpen %}`
    // Must run before `if .` matching.
    if let Some(field) = inner.strip_prefix("if not .") {
        let field = field.trim();
        if !field.is_empty() {
            return Some((format!("{{% if not {field} %}}"), new_i));
        }
    }
    if let Some(field) = inner.strip_prefix("if .") {
        let field = field.trim();
        if !field.is_empty() {
            return Some((format!("{{% if {field} %}}"), new_i));
        }
    }
    if let Some(expr) = inner.strip_prefix('.') {
        let (field, filter) = split_field_filter(expr);
        if let Some(filter) = filter {
            let filter = map_go_filter(filter);
            return Some((format!("{{{{ {field} | {filter} }}}}"), new_i));
        }
        return Some((format!("{{{{ {field} }}}}"), new_i));
    }
    None
}

fn split_field_filter(expr: &str) -> (&str, Option<&str>) {
    if let Some((field, filter)) = expr.split_once(" | ") {
        return (field.trim(), Some(filter.trim()));
    }
    if let Some((field, filter)) = expr.split_once('|') {
        return (field.trim(), Some(filter.trim()));
    }
    (expr.trim(), None)
}

fn map_go_filter(filter: &str) -> &str {
    match filter {
        "cleanDomain" => "clean_domain",
        "formatBytes" => "format_bytes",
        "safeHTML" => "safe_html",
        // stripped in post_process (no-op for our trusted URL contexts)
        "safeURL" => "safe_url",
        other => other,
    }
}

fn post_process(s: &str) -> String {
    let mut s = s.to_string();
    s = s.replace(
        "{{slice .Custom.Name 0 1 | printf \"%s\" | upper}}",
        "{{ Custom.Name[0:1] | upper }}",
    );
    // Go `| safeURL` is a no-op for our templates (URLs are already trusted context).
    s = s.replace(" | safe_url", "");
    s = s.replace("| safe_url", "");
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

    /// Regression: Go `{{if not .Field}}` left unconverted caused Minijinja
    /// `syntax error: unexpected '.', expected in` on custom www_dir pages.
    #[test]
    fn converts_if_not_field() {
        let s = r#"{{if not .RegistrationOpen}}closed{{else}}open{{end}}"#;
        let o = prepare_template(s);
        assert_eq!(
            o,
            "{% if not RegistrationOpen %}closed{% else %}open{% endif %}"
        );
        assert!(looks_like_go_template(s));
        assert!(!looks_like_go_template(&o));
    }

    #[test]
    fn converts_if_not_with_irregular_whitespace() {
        // Production-like and odd spacing operators sometimes paste.
        for s in [
            r#"{{ if not .RegistrationOpen }}closed{{ end }}"#,
            r#"{{if  not  .RegistrationOpen}}closed{{end}}"#,
            r#"{{	if not .RegistrationOpen	}}closed{{	end	}}"#,
        ] {
            let o = prepare_template(s);
            assert!(
                o.contains("{% if not RegistrationOpen %}"),
                "input={s:?} got={o}"
            );
            assert!(o.contains("{% endif %}"), "input={s:?} got={o}");
            assert!(!o.contains("{{if"), "left go markers: {o}");
            assert!(!o.contains(".RegistrationOpen"), "left go dotted if: {o}");
        }
    }

    #[test]
    fn converts_if_not_nested_field() {
        let s = r#"{{if not .Custom.Name}}anon{{else}}named{{end}}"#;
        let o = prepare_template(s);
        assert_eq!(o, "{% if not Custom.Name %}anon{% else %}named{% endif %}");
    }

    #[test]
    fn converts_filter_without_spaces_around_pipe() {
        let s = r#"{{.MailDomain|cleanDomain}}"#;
        let o = prepare_template(s);
        assert_eq!(o, "{{ MailDomain | clean_domain }}");
    }

    #[test]
    fn production_index_snippet_if_not_registration_open() {
        // Mirrors the failing custom index.html form reported by operators.
        let s = r#"      {{if not .RegistrationOpen}}
        <p class="warn">registration closed</p>
      {{else}}
        <p>open</p>
      {{end}}"#;
        let o = prepare_template(s);
        assert!(o.contains("{% if not RegistrationOpen %}"), "got: {o}");
        assert!(o.contains("{% else %}"));
        assert!(o.contains("{% endif %}"));
        assert!(!o.contains("{{if"));
        assert!(!o.contains(".RegistrationOpen"));
        assert!(looks_like_go_template(s));
        assert!(!looks_like_go_template(&o));
    }

    #[test]
    fn leaves_minijinja_unchanged() {
        let s = r#"{% if RegistrationOpen %}{{ MailDomain }}{% endif %}"#;
        assert_eq!(prepare_template(s), s);
        let not_s = r#"{% if not RegistrationOpen %}closed{% endif %}"#;
        assert_eq!(prepare_template(not_s), not_s);
    }

    #[test]
    fn looks_like_go_template_markers() {
        assert!(looks_like_go_template("{{if .X}}"));
        assert!(looks_like_go_template("{{if not .RegistrationOpen}}"));
        assert!(looks_like_go_template("{{ if not .X }}"));
        assert!(looks_like_go_template("{{if  not  .X}}"));
        assert!(looks_like_go_template("{{.MailDomain}}"));
        assert!(looks_like_go_template("x | cleanDomain y"));
        assert!(!looks_like_go_template(
            "{% if X %}{{ MailDomain }}{% endif %}"
        ));
        assert!(!looks_like_go_template(
            "{% if not RegistrationOpen %}closed{% endif %}"
        ));
    }

    #[test]
    fn prepare_template_idempotent() {
        let go = r#"{{if not .RegistrationOpen}}c{{else}}o{{end}} {{.MailDomain|cleanDomain}}"#;
        let once = prepare_template(go);
        let twice = prepare_template(&once);
        assert_eq!(once, twice);
    }
}
