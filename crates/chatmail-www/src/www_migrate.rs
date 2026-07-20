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

//! On-disk migration of custom `www_dir` HTML from Go templates to Minijinja.

use std::fs;
use std::path::{Path, PathBuf};

use chatmail_types::{ChatmailError, Result};
use serde::Serialize;

use crate::go_template::{looks_like_go_template, prepare_template};

/// Result of migrating one HTML file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileMigrateOutcome {
    /// No Go markers or content unchanged after convert.
    Unchanged,
    /// File rewritten; optional backup path.
    Migrated { backup: PathBuf },
}

/// Summary of a `www_dir` migration pass.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MigrateReport {
    pub www_dir: String,
    pub scanned: usize,
    pub go_style_files: Vec<String>,
    pub migrated: Vec<String>,
    pub unchanged: usize,
    pub backups: Vec<String>,
    pub errors: Vec<String>,
}

/// List HTML files under `www_dir` that still look like Go `html/template`.
pub fn scan_www_dir_for_go_templates(www_dir: &Path) -> Result<Vec<PathBuf>> {
    if !www_dir.is_dir() {
        return Err(ChatmailError::config(format!(
            "www_dir is not a directory: {}",
            www_dir.display()
        )));
    }
    let mut out = Vec::new();
    walk_html(www_dir, &mut |path| {
        let src = fs::read_to_string(path).map_err(ChatmailError::from)?;
        if looks_like_go_template(&src) {
            out.push(path.to_path_buf());
        }
        Ok(())
    })?;
    out.sort();
    Ok(out)
}

/// Convert one HTML file in place if it uses Go template syntax.
///
/// Writes `path` with a `.go-template.bak` sibling backup when content changes.
pub fn migrate_www_html_file(path: &Path) -> Result<FileMigrateOutcome> {
    let src = fs::read_to_string(path).map_err(ChatmailError::from)?;
    if !looks_like_go_template(&src) {
        return Ok(FileMigrateOutcome::Unchanged);
    }
    let converted = prepare_template(&src);
    if converted == src {
        return Ok(FileMigrateOutcome::Unchanged);
    }
    let backup = backup_path(path);
    if !backup.exists() {
        fs::write(&backup, &src).map_err(ChatmailError::from)?;
    }
    fs::write(path, converted.as_bytes()).map_err(ChatmailError::from)?;
    Ok(FileMigrateOutcome::Migrated { backup })
}

/// Scan and optionally rewrite all Go-style HTML under `www_dir`.
///
/// When `apply` is false, only scans and fills `go_style_files` (dry run).
pub fn migrate_www_dir(www_dir: &Path, apply: bool) -> Result<MigrateReport> {
    let mut report = MigrateReport {
        www_dir: www_dir.display().to_string(),
        ..Default::default()
    };

    if !www_dir.is_dir() {
        return Err(ChatmailError::config(format!(
            "www_dir is not a directory: {}",
            www_dir.display()
        )));
    }

    let mut all_html = Vec::new();
    walk_html(www_dir, &mut |path| {
        all_html.push(path.to_path_buf());
        Ok(())
    })?;
    report.scanned = all_html.len();

    for path in all_html {
        let rel = path
            .strip_prefix(www_dir)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.display().to_string());
        let src = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                report
                    .errors
                    .push(format!("{}: read failed: {e}", path.display()));
                continue;
            }
        };
        if !looks_like_go_template(&src) {
            report.unchanged += 1;
            continue;
        }
        report.go_style_files.push(rel.clone());
        if !apply {
            continue;
        }
        match migrate_www_html_file(&path) {
            Ok(FileMigrateOutcome::Migrated { backup }) => {
                report.migrated.push(rel);
                report.backups.push(backup.display().to_string());
            }
            Ok(FileMigrateOutcome::Unchanged) => {
                report.unchanged += 1;
            }
            Err(e) => {
                report.errors.push(format!("{}: {e}", path.display()));
            }
        }
    }

    Ok(report)
}

fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".go-template.bak");
    PathBuf::from(s)
}

fn walk_html(dir: &Path, f: &mut dyn FnMut(&Path) -> Result<()>) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(ChatmailError::from)?;
    for entry in entries {
        let entry = entry.map_err(ChatmailError::from)?;
        let path = entry.path();
        if path.is_dir() {
            walk_html(&path, f)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("html"))
        {
            f(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::go_template::looks_like_go_template;

    #[test]
    fn detect_go_vs_minijinja() {
        assert!(looks_like_go_template(
            r#"{{if .RegistrationOpen}}yes{{else}}no{{end}} {{.MailDomain | cleanDomain}}"#
        ));
        assert!(!looks_like_go_template(
            r#"{% if RegistrationOpen %}yes{% else %}no{% endif %} {{ MailDomain | clean_domain }}"#
        ));
        assert!(looks_like_go_template(r#"hello {{.WebDomain}}"#));
        assert!(!looks_like_go_template(r#"hello {{ WebDomain }}"#));
        // Operator custom pages often use `if not` (Go html/template).
        assert!(looks_like_go_template(
            r#"{{if not .RegistrationOpen}}registration closed{{end}}"#
        ));
    }

    /// On-disk HTML with only `{{if not .Field}}` must convert and render closed/open correctly.
    #[test]
    fn migrate_if_not_registration_open_renders() {
        use crate::template::{TemplateEngine, WwwContext};
        use chatmail_config::AppConfig;

        let dir = tempfile::tempdir().unwrap();
        // Minimal snippet matching the failure mode (unexpected '.' in Minijinja).
        let go = r#"<!DOCTYPE html><body>
{{if not .RegistrationOpen}}registration is closed{{else}}registration is open{{end}}
</body></html>"#;
        fs::write(dir.path().join("index.html"), go).unwrap();

        let cfg = AppConfig {
            www_dir: Some(dir.path().to_path_buf()),
            mail_domain: Some("example.org".into()),
            ..Default::default()
        };
        let ctx_closed = WwwContext {
            MailDomain: "example.org".into(),
            MXDomain: String::new(),
            WebDomain: "example.org".into(),
            PublicIP: String::new(),
            Version: "test".into(),
            RegistrationOpen: false,
            JitRegistrationEnabled: false,
            Language: "en".into(),
            ClientHost: String::new(),
            ImapPortTLS: "993".into(),
            ImapPortStartTLS: "143".into(),
            SmtpPortTLS: "465".into(),
            SmtpPortStartTLS: "587".into(),
            DcloginImapSecurity: String::new(),
            DcloginSmtpSecurity: String::new(),
            DefaultQuota: 0,
            SSURL: String::new(),
            V2rayNGConfigWS: String::new(),
            V2rayNGConfigGRPC: String::new(),
            MessageRetentionLine: None,
            Custom: None,
        };
        let mut ctx_open = ctx_closed.clone();
        ctx_open.RegistrationOpen = true;

        // Runtime prepare_template path (no on-disk migrate yet).
        let engine = TemplateEngine::from_config(&cfg);
        let rendered = engine.render("index.html", &ctx_closed).unwrap();
        assert!(
            rendered.contains("registration is closed"),
            "got: {rendered}"
        );
        assert!(!rendered.contains("{{if"));
        let rendered_open = engine.render("index.html", &ctx_open).unwrap();
        assert!(
            rendered_open.contains("registration is open"),
            "got: {rendered_open}"
        );

        // On-disk migrate then re-render both branches.
        let report = migrate_www_dir(dir.path(), true).unwrap();
        assert_eq!(report.migrated.len(), 1);
        let disk = fs::read_to_string(dir.path().join("index.html")).unwrap();
        assert!(
            disk.contains("{% if not RegistrationOpen %}"),
            "disk: {disk}"
        );
        assert!(!looks_like_go_template(&disk));

        let engine2 = TemplateEngine::from_config(&cfg);
        let after = engine2.render("index.html", &ctx_closed).unwrap();
        assert!(after.contains("registration is closed"), "got: {after}");
        let after_open = engine2.render("index.html", &ctx_open).unwrap();
        assert!(
            after_open.contains("registration is open"),
            "got: {after_open}"
        );

        // Second migrate is a no-op.
        let again = migrate_www_dir(dir.path(), true).unwrap();
        assert!(again.migrated.is_empty(), "expected idempotent migrate");
    }

    #[test]
    fn migrate_detects_if_not_only_file() {
        let dir = tempfile::tempdir().unwrap();
        // File with ONLY `if not` (no `{{if .` / `{{.Field}}`) must still be detected.
        fs::write(
            dir.path().join("closed.html"),
            r#"{{if not .RegistrationOpen}}closed{{end}}"#,
        )
        .unwrap();
        let found = scan_www_dir_for_go_templates(dir.path()).unwrap();
        assert_eq!(found.len(), 1);
        let report = migrate_www_dir(dir.path(), true).unwrap();
        assert_eq!(report.migrated.len(), 1);
        let body = fs::read_to_string(dir.path().join("closed.html")).unwrap();
        assert_eq!(body, "{% if not RegistrationOpen %}closed{% endif %}");
    }

    #[test]
    fn migrate_file_writes_backup_and_converts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.html");
        let go = r#"<!DOCTYPE html><body>{{if .RegistrationOpen}}open{{else}}closed{{end}} {{.MailDomain | cleanDomain}}</body>"#;
        fs::write(&path, go).unwrap();

        let out = migrate_www_html_file(&path).unwrap();
        assert!(matches!(out, FileMigrateOutcome::Migrated { .. }));
        let bak = backup_path(&path);
        assert!(bak.is_file(), "backup missing: {}", bak.display());
        assert_eq!(fs::read_to_string(&bak).unwrap(), go);

        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("{% if RegistrationOpen %}"));
        assert!(body.contains("{{ MailDomain | clean_domain }}"));
        assert!(!looks_like_go_template(&body));

        // Second pass is a no-op.
        assert_eq!(
            migrate_www_html_file(&path).unwrap(),
            FileMigrateOutcome::Unchanged
        );
    }

    #[test]
    fn migrate_dir_dry_run_and_apply() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.html"), r#"{{if .SSURL}}ss{{end}}"#).unwrap();
        fs::write(dir.path().join("b.html"), r#"{{ MailDomain }}"#).unwrap();
        fs::write(dir.path().join("note.txt"), "not html").unwrap();

        let dry = migrate_www_dir(dir.path(), false).unwrap();
        assert_eq!(dry.scanned, 2);
        assert_eq!(dry.go_style_files.len(), 1);
        assert!(dry.migrated.is_empty());
        // File still Go-style after dry run.
        assert!(looks_like_go_template(
            &fs::read_to_string(dir.path().join("a.html")).unwrap()
        ));

        let applied = migrate_www_dir(dir.path(), true).unwrap();
        assert_eq!(applied.migrated.len(), 1);
        assert!(!looks_like_go_template(
            &fs::read_to_string(dir.path().join("a.html")).unwrap()
        ));
        assert!(!looks_like_go_template(
            &fs::read_to_string(dir.path().join("b.html")).unwrap()
        ));
    }

    #[test]
    fn scan_lists_go_files_only() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.html"), r#"{{.X}}"#).unwrap();
        fs::write(dir.path().join("mj.html"), r#"{{ X }}"#).unwrap();
        let found = scan_www_dir_for_go_templates(dir.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("go.html"));
    }

    /// Migrated on-disk HTML must still render via TemplateEngine (prepare is idempotent).
    #[test]
    fn migrate_then_render_matches_go_path() {
        use crate::template::{TemplateEngine, WwwContext};
        use chatmail_config::AppConfig;

        let dir = tempfile::tempdir().unwrap();
        let go = r#"<!DOCTYPE html>
<html lang="{{.Language}}" dir="{{if eq .Language "fa"}}rtl{{else}}ltr{{end}}">
<body>
{{if .RegistrationOpen}}open{{else}}closed{{end}}
<span>{{.MailDomain | cleanDomain}}</span>
</body></html>"#;
        fs::write(dir.path().join("index.html"), go).unwrap();

        // Before migrate: external www still renders Go syntax via prepare_template.
        let cfg = AppConfig {
            www_dir: Some(dir.path().to_path_buf()),
            mail_domain: Some("example.org".into()),
            ..Default::default()
        };
        let engine = TemplateEngine::from_config(&cfg);
        let ctx = WwwContext {
            MailDomain: "example.org".into(),
            MXDomain: String::new(),
            WebDomain: "example.org".into(),
            PublicIP: String::new(),
            Version: "test".into(),
            RegistrationOpen: true,
            JitRegistrationEnabled: false,
            Language: "en".into(),
            ClientHost: String::new(),
            ImapPortTLS: "993".into(),
            ImapPortStartTLS: "143".into(),
            SmtpPortTLS: "465".into(),
            SmtpPortStartTLS: "587".into(),
            DcloginImapSecurity: String::new(),
            DcloginSmtpSecurity: String::new(),
            DefaultQuota: 0,
            SSURL: String::new(),
            V2rayNGConfigWS: String::new(),
            V2rayNGConfigGRPC: String::new(),
            MessageRetentionLine: None,
            Custom: None,
        };
        let before = engine.render("index.html", &ctx).unwrap();
        assert!(before.contains("open"), "got: {before}");
        assert!(before.contains("example.org"), "got: {before}");
        assert!(
            before.contains(r#"dir="ltr""#) || before.contains("dir=\"ltr\""),
            "got: {before}"
        );

        migrate_www_dir(dir.path(), true).unwrap();
        let after_src = fs::read_to_string(dir.path().join("index.html")).unwrap();
        assert!(!looks_like_go_template(&after_src), "still go: {after_src}");
        assert!(after_src.contains("{% if RegistrationOpen %}"));

        // Engine re-reads disk; prepare_template on Minijinja is a no-op.
        let engine2 = TemplateEngine::from_config(&cfg);
        let after = engine2.render("index.html", &ctx).unwrap();
        assert!(after.contains("open"), "got: {after}");
        assert!(after.contains("example.org"), "got: {after}");
        // Rendered output should not still contain Go markers.
        assert!(!after.contains("{{if"));
        assert!(!after.contains("{{."));
    }

    #[test]
    fn prepare_template_idempotent_on_migrated_and_go() {
        let go = r#"{{if .RegistrationOpen}}y{{else}}n{{end}} {{.MailDomain | cleanDomain}}"#;
        let once = prepare_template(go);
        let twice = prepare_template(&once);
        assert_eq!(once, twice);
        assert!(!looks_like_go_template(&once));
    }

    #[test]
    fn migrate_does_not_touch_non_html() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("app.js"), r#"const x = "{{if .X}}";"#).unwrap();
        fs::write(dir.path().join("main.css"), "/* {{.Foo}} */").unwrap();
        let report = migrate_www_dir(dir.path(), true).unwrap();
        assert_eq!(report.scanned, 0);
        assert_eq!(
            fs::read_to_string(dir.path().join("app.js")).unwrap(),
            r#"const x = "{{if .X}}";"#
        );
    }
}
