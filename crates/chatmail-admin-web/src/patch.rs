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

/// Rewrite `index.html` so the SPA works under a non-root path (Madmail `serveAdminWeb`).
pub fn patch_index_html(index: &str, prefix: &str) -> Vec<u8> {
    let mut prefix_with_slash = prefix.trim_end_matches('/').to_string();
    if !prefix_with_slash.starts_with('/') {
        prefix_with_slash.insert(0, '/');
    }
    if !prefix_with_slash.ends_with('/') {
        prefix_with_slash.push('/');
    }
    let clean_prefix = prefix_with_slash.trim_end_matches('/');

    let mut patched = index.to_string();
    patched = patched.replace("href=\"/", &format!("href=\"{prefix_with_slash}"));
    patched = patched.replace("src=\"/", &format!("src=\"{prefix_with_slash}"));
    patched = patched.replace("import(\"/", &format!("import(\"{prefix_with_slash}"));
    patched = patched.replace(r#"base: """#, &format!(r#"base: "{clean_prefix}""#));

    let inject = format!(
        r#"<script>window.__MADMAIL_ADMIN_PREFIX__="{clean_prefix}";window.__MADMAIL_API_DEFAULT__="/api/admin";</script>"#
    );
    if let Some(head_end) = patched.find("</head>") {
        patched.insert_str(head_end, &inject);
    } else {
        patched.insert_str(0, &inject);
    }

    patched.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_base_and_assets() {
        let html = r#"<link href="/_app/x.js"><script>base: ""</script>"#;
        let out = String::from_utf8(patch_index_html(html, "/admin")).unwrap();
        assert!(out.contains(r#"href="/admin/_app/x.js""#));
        assert!(out.contains(r#"base: "/admin""#));
        assert!(out.contains(r#"__MADMAIL_ADMIN_PREFIX__="/admin""#));
        assert!(out.contains(r#"__MADMAIL_API_DEFAULT__="/api/admin""#));
    }
}
