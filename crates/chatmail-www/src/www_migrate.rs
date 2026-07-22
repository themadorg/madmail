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

//! On-disk migration of custom `www_dir`:
//! - Go `html/template` → Minijinja
//! - Legacy `/qr?data=` image assignments → client-side `setQrCodeImage`
//! - Warnings for literal `{%` / `{{` that can break Minijinja (e.g. Obtainium URLs)

use std::fs;
use std::path::{Path, PathBuf};

use chatmail_types::{ChatmailError, Result};
use serde::Serialize;

use crate::assets::WwwAssets;
use crate::go_template::{looks_like_go_template, prepare_template};

/// Client-side QR helper shipped in embedded `main.js` (appended when missing).
const SET_QR_CODE_IMAGE_JS: &str = r#"
/** Render a dclogin / invite QR into an <img> (client-side, no /qr backend). */
function setQrCodeImage(imgEl, text, cellSize) {
    if (!imgEl || !text || typeof qrcode !== 'function') {
        return;
    }
    try {
        var qr = qrcode(0, 'M');
        qr.addData(text);
        qr.make();
        imgEl.src = qr.createDataURL(cellSize || 4, 2);
        imgEl.alt = 'QR Code';
    } catch (err) {
        console.error('QR generation failed', err);
    }
}
"#;

/// Result of migrating one HTML file (Go syntax and/or QR).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileMigrateOutcome {
    /// No Go markers / QR legacy / content unchanged after convert.
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
    /// HTML/JS paths that still referenced legacy `/qr?data=` (before apply) or were QR-fixed.
    pub qr_legacy_files: Vec<String>,
    /// Files rewritten for client-side QR (HTML and/or `main.js`).
    pub qr_migrated: Vec<String>,
    /// `qrcode.min.js` was copied from embedded assets into `www_dir`.
    pub qrcode_js_copied: bool,
    /// `main.js` gained `setQrCodeImage` (appended).
    pub main_js_qr_helper_added: bool,
    /// Suspicious literal `{%` / `{{` that may break Minijinja (not auto-fixed).
    pub literal_brace_warnings: Vec<String>,
}

impl MigrateReport {
    /// Dry-run: would apply rewrite something (or copy assets)?
    pub fn dry_needs_apply(&self) -> bool {
        !self.go_style_files.is_empty() || !self.qr_legacy_files.is_empty()
    }
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

/// Convert one HTML file in place if it uses Go template syntax and/or legacy QR.
///
/// Writes `path` with a `.go-template.bak` sibling backup when content changes.
pub fn migrate_www_html_file(path: &Path) -> Result<FileMigrateOutcome> {
    let src = fs::read_to_string(path).map_err(ChatmailError::from)?;
    let converted = transform_html_source(&src);
    if converted == src {
        return Ok(FileMigrateOutcome::Unchanged);
    }
    let backup = backup_path(path, ".go-template.bak");
    if !backup.exists() {
        fs::write(&backup, &src).map_err(ChatmailError::from)?;
    }
    fs::write(path, converted.as_bytes()).map_err(ChatmailError::from)?;
    Ok(FileMigrateOutcome::Migrated { backup })
}

/// Go convert + QR rewrite for one HTML body (no I/O).
pub fn transform_html_source(src: &str) -> String {
    let mut s = src.to_string();
    if looks_like_go_template(&s) {
        s = prepare_template(&s);
    }
    rewrite_legacy_qr_js(&s)
}

/// Scan and optionally rewrite all Go-style HTML and QR/static assets under `www_dir`.
///
/// When `apply` is false, only scans (dry run): fills `go_style_files`, `qr_legacy_files`,
/// and `literal_brace_warnings` without writing.
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

    for path in &all_html {
        let rel = rel_under(www_dir, path);
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                report
                    .errors
                    .push(format!("{}: read failed: {e}", path.display()));
                continue;
            }
        };

        for w in scan_literal_brace_warnings(&src, &rel) {
            report.literal_brace_warnings.push(w);
        }

        let has_go = looks_like_go_template(&src);
        let has_qr = source_has_legacy_qr(&src);
        if has_go {
            report.go_style_files.push(rel.clone());
        }
        if has_qr {
            report.qr_legacy_files.push(rel.clone());
        }

        if !has_go && !has_qr {
            report.unchanged += 1;
            continue;
        }
        if !apply {
            continue;
        }

        let converted = transform_html_source(&src);
        if converted == src {
            report.unchanged += 1;
            continue;
        }
        let backup = backup_path(path, ".go-template.bak");
        if !backup.exists() {
            if let Err(e) = fs::write(&backup, &src) {
                report
                    .errors
                    .push(format!("{}: backup failed: {e}", path.display()));
                continue;
            }
        }
        if let Err(e) = fs::write(path, converted.as_bytes()) {
            report
                .errors
                .push(format!("{}: write failed: {e}", path.display()));
            continue;
        }
        report.migrated.push(rel.clone());
        report.backups.push(backup.display().to_string());
        if has_qr && !source_has_legacy_qr(&converted) {
            report.qr_migrated.push(rel);
        }
    }

    // `main.js` may still assign `/qr?data=` or lack setQrCodeImage.
    migrate_main_js(www_dir, apply, &mut report)?;

    // Ensure qrcode.min.js when pages need client QR (legacy or setQrCodeImage).
    ensure_qrcode_min_js(www_dir, apply, &mut report)?;

    // Inject <script src="…qrcode.min.js"> before main.js when missing.
    if apply {
        inject_qrcode_script_tags(www_dir, &mut report)?;
    } else {
        // Dry-run: note HTML that reference setQrCodeImage/legacy QR but lack qrcode script.
        for path in &all_html {
            let rel = rel_under(www_dir, path);
            let Ok(src) = fs::read_to_string(path) else {
                continue;
            };
            if needs_qrcode_script(&src)
                && !html_loads_qrcode_js(&src)
                && !report.qr_legacy_files.contains(&rel)
            {
                report.qr_legacy_files.push(rel);
            }
        }
        let main_js = www_dir.join("main.js");
        if main_js.is_file() {
            if let Ok(src) = fs::read_to_string(&main_js) {
                if source_has_legacy_qr(&src) || !src.contains("function setQrCodeImage") {
                    // If any html has QR usage and helper missing, flag main.js
                    let any_qr_html = report.qr_legacy_files.iter().any(|f| f.ends_with(".html"));
                    let any_set_qr = all_html.iter().any(|p| {
                        fs::read_to_string(p)
                            .map(|s| s.contains("setQrCodeImage(") || source_has_legacy_qr(&s))
                            .unwrap_or(false)
                    });
                    if (any_qr_html || any_set_qr || source_has_legacy_qr(&src))
                        && !report.qr_legacy_files.iter().any(|f| f == "main.js")
                    {
                        report.qr_legacy_files.push("main.js".into());
                    }
                }
            }
        }
        if !www_dir.join("qrcode.min.js").is_file() {
            let needs = !report.qr_legacy_files.is_empty()
                || all_html.iter().any(|p| {
                    fs::read_to_string(p)
                        .map(|s| s.contains("setQrCodeImage(") || source_has_legacy_qr(&s))
                        .unwrap_or(false)
                });
            if needs {
                // Signal work via qr_legacy placeholder when only asset missing
                if report.qr_legacy_files.is_empty() {
                    report.qr_legacy_files.push("qrcode.min.js".into());
                }
            }
        }
    }

    Ok(report)
}

fn migrate_main_js(www_dir: &Path, apply: bool, report: &mut MigrateReport) -> Result<()> {
    let path = www_dir.join("main.js");
    if !path.is_file() {
        // No main.js — only care if HTML still has legacy QR after transform intent
        return Ok(());
    }
    let src = fs::read_to_string(&path).map_err(ChatmailError::from)?;
    let mut next = rewrite_legacy_qr_js(&src);
    let mut helper_added = false;
    if !next.contains("function setQrCodeImage") {
        // Append helper when pages need it (legacy QR or call sites).
        let html_needs = report.qr_legacy_files.iter().any(|f| f.ends_with(".html"))
            || walk_html_any(www_dir, |p| {
                fs::read_to_string(p)
                    .map(|s| s.contains("setQrCodeImage(") || source_has_legacy_qr(&s))
                    .unwrap_or(false)
            });
        if html_needs || source_has_legacy_qr(&src) || next.contains("setQrCodeImage(") {
            if !next.ends_with('\n') {
                next.push('\n');
            }
            next.push_str(SET_QR_CODE_IMAGE_JS);
            helper_added = true;
        }
    }

    let changed = next != src;
    if (source_has_legacy_qr(&src) || helper_added)
        && !report.qr_legacy_files.iter().any(|f| f == "main.js")
    {
        report.qr_legacy_files.push("main.js".into());
    }
    if !changed {
        return Ok(());
    }
    if !apply {
        return Ok(());
    }

    let backup = backup_path(&path, ".qr-compat.bak");
    if !backup.exists() {
        fs::write(&backup, &src).map_err(ChatmailError::from)?;
        report.backups.push(backup.display().to_string());
    }
    fs::write(&path, next.as_bytes()).map_err(ChatmailError::from)?;
    report.qr_migrated.push("main.js".into());
    if helper_added {
        report.main_js_qr_helper_added = true;
    }
    Ok(())
}

fn ensure_qrcode_min_js(www_dir: &Path, apply: bool, report: &mut MigrateReport) -> Result<()> {
    let dest = www_dir.join("qrcode.min.js");
    if dest.is_file() {
        return Ok(());
    }
    let needs = report
        .qr_legacy_files
        .iter()
        .any(|f| f.ends_with(".html") || f == "main.js")
        || report.qr_migrated.iter().any(|f| f.ends_with(".html"))
        || report.main_js_qr_helper_added
        || walk_html_any(www_dir, |p| {
            fs::read_to_string(p)
                .map(|s| s.contains("setQrCodeImage(") || source_has_legacy_qr(&s))
                .unwrap_or(false)
        });
    if !needs {
        return Ok(());
    }
    if !apply {
        if !report.qr_legacy_files.iter().any(|f| f == "qrcode.min.js") {
            report.qr_legacy_files.push("qrcode.min.js".into());
        }
        return Ok(());
    }
    let Some(file) = WwwAssets::get("qrcode.min.js") else {
        report
            .errors
            .push("embedded qrcode.min.js missing from binary".into());
        return Ok(());
    };
    fs::write(&dest, file.data.as_ref()).map_err(ChatmailError::from)?;
    report.qrcode_js_copied = true;
    report.qr_migrated.push("qrcode.min.js".into());
    Ok(())
}

fn inject_qrcode_script_tags(www_dir: &Path, report: &mut MigrateReport) -> Result<()> {
    let mut all_html = Vec::new();
    walk_html(www_dir, &mut |path| {
        all_html.push(path.to_path_buf());
        Ok(())
    })?;
    for path in all_html {
        let src = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !needs_qrcode_script(&src) || html_loads_qrcode_js(&src) {
            continue;
        }
        let Some(next) = inject_qrcode_script_before_main_js(&src) else {
            continue;
        };
        if next == src {
            continue;
        }
        let backup = backup_path(&path, ".go-template.bak");
        if !backup.exists() {
            let _ = fs::write(&backup, &src);
            report.backups.push(backup.display().to_string());
        }
        fs::write(&path, next.as_bytes()).map_err(ChatmailError::from)?;
        let rel = rel_under(www_dir, &path);
        if !report.qr_migrated.contains(&rel) {
            report.qr_migrated.push(rel.clone());
        }
        if !report.migrated.contains(&rel) {
            report.migrated.push(rel);
        }
    }
    Ok(())
}

fn needs_qrcode_script(src: &str) -> bool {
    // Call site only — avoid injecting into docs that merely mention the name.
    src.contains("setQrCodeImage(") || source_has_legacy_qr(src)
}

fn html_loads_qrcode_js(src: &str) -> bool {
    src.contains("qrcode.min.js")
}

/// Insert `<script src="…qrcode.min.js">` immediately before the first `main.js` script tag.
fn inject_qrcode_script_before_main_js(src: &str) -> Option<String> {
    // Match common forms: <script src="./main.js"></script> or src="/main.js" or 'main.js'
    let markers = [
        r#"src="./main.js""#,
        r#"src='/main.js'"#,
        r#"src="/main.js""#,
        r#"src='./main.js'"#,
        r#"src="main.js""#,
        r#"src='main.js'"#,
    ];
    let mut best: Option<usize> = None;
    for m in markers {
        if let Some(i) = src.find(m) {
            best = Some(match best {
                Some(b) => b.min(i),
                None => i,
            });
        }
    }
    let idx = best?;
    // Walk back to start of <script
    let head = &src[..idx];
    let script_start = head.rfind("<script")?;
    // Detect path style from main.js src
    let main_attr = &src[idx..];
    let qr_src = if main_attr.contains("\"./main.js\"") || main_attr.contains("'./main.js'") {
        "./qrcode.min.js"
    } else if main_attr.contains("\"/main.js\"") || main_attr.contains("'/main.js'") {
        "/qrcode.min.js"
    } else {
        "./qrcode.min.js"
    };
    let indent = {
        let line_start = head.rfind('\n').map(|i| i + 1).unwrap_or(0);
        src[line_start..script_start]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect::<String>()
    };
    let injection = format!("{indent}<script src=\"{qr_src}\"></script>\n{indent}");
    let mut out = String::with_capacity(src.len() + injection.len());
    out.push_str(&src[..script_start]);
    out.push_str(&injection);
    out.push_str(&src[script_start..]);
    Some(out)
}

/// True when this source still has a **rewriteable** legacy `/qr?data=` assignment.
///
/// Plain mentions in docs/prose (e.g. “the old `/qr?data=` endpoint”) do **not** count —
/// otherwise migrate would spuriously prompt and inject `qrcode.min.js` into unrelated pages.
fn source_has_legacy_qr(src: &str) -> bool {
    if !src.contains("/qr?data=") {
        return false;
    }
    rewrite_legacy_qr_js(src) != src
}

/// Rewrite `el.src = `/qr?data=${encodeURIComponent(var)}`` → `setQrCodeImage(el, var)`.
pub fn rewrite_legacy_qr_js(src: &str) -> String {
    let mut s = src.to_string();
    // Fast paths for the common Madmail index/invite snippets.
    const PAIRS: &[(&str, &str)] = &[
        (
            r#"document.getElementById("result-qr").src = `/qr?data=${encodeURIComponent(currentLink)}`;"#,
            r#"setQrCodeImage(document.getElementById("result-qr"), currentLink);"#,
        ),
        (
            r#"document.getElementById('result-qr').src = `/qr?data=${encodeURIComponent(currentLink)}`;"#,
            r#"setQrCodeImage(document.getElementById('result-qr'), currentLink);"#,
        ),
        (
            r#"document.getElementById("result-qr").src = `/qr?data=${encodeURIComponent(currentLink)}`"#,
            r#"setQrCodeImage(document.getElementById("result-qr"), currentLink)"#,
        ),
        (
            r#"document.getElementById('result-qr').src = `/qr?data=${encodeURIComponent(currentLink)}`"#,
            r#"setQrCodeImage(document.getElementById('result-qr'), currentLink)"#,
        ),
    ];
    for (from, to) in PAIRS {
        s = s.replace(from, to);
    }
    // Generic: any getElementById(...).src = `/qr?data=${encodeURIComponent(VAR)}`
    loop {
        let next = rewrite_one_backtick_qr_assignment(&s);
        if next == s {
            break;
        }
        s = next;
    }
    s
}

fn rewrite_one_backtick_qr_assignment(src: &str) -> String {
    const MARK: &str = "/qr?data=${encodeURIComponent(";
    let Some(mark_at) = src.find(MARK) else {
        return src.to_string();
    };
    let after_mark = &src[mark_at + MARK.len()..];
    let Some(var_end) = after_mark.find(')') else {
        return src.to_string();
    };
    let var = after_mark[..var_end].trim();
    if var.is_empty()
        || !var
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        // Skip this occurrence: advance past MARK so we do not loop forever.
        let mut out = String::with_capacity(src.len());
        out.push_str(&src[..mark_at + MARK.len()]);
        out.push_str(&rewrite_one_backtick_qr_assignment(
            &src[mark_at + MARK.len()..],
        ));
        return out;
    }
    let after_var = &after_mark[var_end..];
    if !after_var.starts_with(")}") {
        let mut out = String::with_capacity(src.len());
        out.push_str(&src[..mark_at + 1]);
        out.push_str(&rewrite_one_backtick_qr_assignment(&src[mark_at + 1..]));
        return out;
    }
    let mut end = mark_at + MARK.len() + var_end + 2; // through `)}
    if src[end..].starts_with(';') {
        end += 1;
    }

    // Walk back: optional whitespace, backtick, `=`, `.src`, getElementById(...)
    let mut i = mark_at;
    while i > 0 && src.as_bytes()[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    if i == 0 || src.as_bytes()[i - 1] != b'`' {
        let mut out = String::with_capacity(src.len());
        out.push_str(&src[..mark_at + 1]);
        out.push_str(&rewrite_one_backtick_qr_assignment(&src[mark_at + 1..]));
        return out;
    }
    i -= 1; // backtick
    while i > 0 && src.as_bytes()[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    if i == 0 || src.as_bytes()[i - 1] != b'=' {
        let mut out = String::with_capacity(src.len());
        out.push_str(&src[..mark_at + 1]);
        out.push_str(&rewrite_one_backtick_qr_assignment(&src[mark_at + 1..]));
        return out;
    }
    i -= 1; // =
    while i > 0 && src.as_bytes()[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    if i < 4 || &src[i - 4..i] != ".src" {
        let mut out = String::with_capacity(src.len());
        out.push_str(&src[..mark_at + 1]);
        out.push_str(&rewrite_one_backtick_qr_assignment(&src[mark_at + 1..]));
        return out;
    }
    let expr_end = i - 4; // before .src
    let head = &src[..expr_end];
    let Some(ge) = head.rfind("document.getElementById(") else {
        let mut out = String::with_capacity(src.len());
        out.push_str(&src[..mark_at + 1]);
        out.push_str(&rewrite_one_backtick_qr_assignment(&src[mark_at + 1..]));
        return out;
    };
    // Between ge and expr_end should only be the call (+ whitespace).
    let call = &src[ge..expr_end].trim_end();
    if !is_balanced_get_element_by_id(call) {
        let mut out = String::with_capacity(src.len());
        out.push_str(&src[..mark_at + 1]);
        out.push_str(&rewrite_one_backtick_qr_assignment(&src[mark_at + 1..]));
        return out;
    }
    let semi = if end > 0 && src.as_bytes()[end - 1] == b';' {
        ";"
    } else {
        ""
    };
    let mut out = String::with_capacity(src.len());
    out.push_str(&src[..ge]);
    out.push_str(&format!("setQrCodeImage({call}, {var}){semi}"));
    out.push_str(&src[end..]);
    out
}

fn is_balanced_get_element_by_id(call: &str) -> bool {
    if !call.starts_with("document.getElementById(") {
        return false;
    }
    let mut depth = 0i32;
    for (i, c) in call.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return call[i + c.len_utf8()..].trim().is_empty();
                }
            }
            _ => {}
        }
    }
    false
}

fn backup_path(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

fn rel_under(www_dir: &Path, path: &Path) -> String {
    path.strip_prefix(www_dir)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
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

fn walk_html_any(dir: &Path, mut pred: impl FnMut(&Path) -> bool) -> bool {
    let mut found = false;
    let _ = walk_html(dir, &mut |path| {
        if pred(path) {
            found = true;
        }
        Ok(())
    });
    found
}

/// Known Minijinja (and leftover Go-control) tag names after `{%` / `{%-`.
const KNOWN_TAG_STARTS: &[&str] = &[
    "if",
    "else",
    "elif",
    "endif",
    "for",
    "endfor",
    "raw",
    "endraw",
    "block",
    "endblock",
    "set",
    "include",
    "extends",
    "macro",
    "endmacro",
    "filter",
    "endfilter",
    "with",
    "endwith",
    "import",
    "from",
    "call",
    "endcall",
    "autoescape",
    "endautoescape",
    "do",
    "break",
    "continue",
];

/// Scan template source for `{%` / `{{` that look like literal content (e.g. `{%22` in URLs).
pub fn scan_literal_brace_warnings(src: &str, rel_path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && (bytes[i + 1] == b'%' || bytes[i + 1] == b'{') {
            let kind = if bytes[i + 1] == b'%' { "{%" } else { "{{" };
            let rest = &src[i + 2..];
            let rest_trim = rest.trim_start_matches(|c: char| c == '-' || c.is_whitespace());
            let ok = if kind == "{%" {
                looks_like_known_tag(rest_trim)
            } else {
                // `{{` variable/expression: letter, `_`, or filter-ish start; not digits alone like `{{22`
                looks_like_known_var_expr(rest_trim)
            };
            if !ok {
                let line = src[..i].bytes().filter(|&b| b == b'\n').count() + 1;
                let snippet: String = src[i..].chars().take(40).collect();
                let snippet = snippet.replace('\n', " ");
                out.push(format!(
                    "{rel_path}:{line}: possible literal `{kind}` in content ({snippet}…). \
                     Wrap with {{% raw %}}…{{% endraw %}} if this is a URL or plain text \
                     (e.g. Obtainium links with {{%22)."
                ));
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    out
}

fn looks_like_known_tag(rest: &str) -> bool {
    for tag in KNOWN_TAG_STARTS {
        if let Some(after) = rest.strip_prefix(tag) {
            if after.is_empty()
                || after.starts_with(|c: char| c.is_whitespace() || c == '%' || c == '-')
            {
                return true;
            }
        }
    }
    false
}

fn looks_like_known_var_expr(rest: &str) -> bool {
    let rest = rest.trim_start();
    if rest.is_empty() {
        return false;
    }
    // Valid: Field, .Field (Go, converted later), _, space then name, end `}}`
    let c = rest.chars().next().unwrap();
    if c.is_ascii_alphabetic() || c == '_' || c == '.' {
        return true;
    }
    // Filters / paren expressions rare at start; reject digits (e.g. broken) and quotes
    false
}

/// Enrich a Minijinja / template engine error for operators.
pub fn format_www_template_error(file: &str, err: &str) -> String {
    let mut msg = format!("www template render failed for {file}: {err}");
    let lower = err.to_ascii_lowercase();
    if lower.contains("syntax")
        || lower.contains("unexpected")
        || lower.contains("unknown")
        || lower.contains("parse")
        || lower.contains("invalid")
    {
        msg.push_str(
            ". Hint: Minijinja treats `{%` and `{{` as template syntax — if this page embeds \
             literal URLs containing those sequences (e.g. Obtainium `{%22…`), wrap them in \
             `{% raw %}…{% endraw %}`. After a Go Madmail upgrade, run \
             `madmail html-migrate --yes` for Go→Minijinja and legacy `/qr?data=` → client-side QR.",
        );
    }
    msg
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

        // Second migrate is a no-op for Go templates.
        let again = migrate_www_dir(dir.path(), true).unwrap();
        assert!(
            again.go_style_files.is_empty() && again.migrated.is_empty(),
            "expected idempotent migrate: {:?}",
            again
        );
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
        let bak = backup_path(&path, ".go-template.bak");
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

    #[test]
    fn rewrite_qr_assignment_common_forms() {
        let old = r#"document.getElementById("result-qr").src = `/qr?data=${encodeURIComponent(currentLink)}`;"#;
        let new = rewrite_legacy_qr_js(old);
        assert_eq!(
            new,
            r#"setQrCodeImage(document.getElementById("result-qr"), currentLink);"#
        );
        let old2 =
            r#"document.getElementById('result-qr').src = `/qr?data=${encodeURIComponent(link)}`;"#;
        let new2 = rewrite_legacy_qr_js(old2);
        assert!(new2.contains("setQrCodeImage"), "{new2}");
        assert!(!new2.contains("/qr?data="), "{new2}");
    }

    #[test]
    fn migrate_qr_legacy_html_and_assets() {
        let dir = tempfile::tempdir().unwrap();
        let html = r#"<!DOCTYPE html>
<html><head>
<script src="./main.js"></script>
</head><body>
<script>
document.getElementById("result-qr").src = `/qr?data=${encodeURIComponent(currentLink)}`;
</script>
</body></html>"#;
        fs::write(dir.path().join("index.html"), html).unwrap();
        fs::write(dir.path().join("main.js"), "function noop() {}\n").unwrap();

        let dry = migrate_www_dir(dir.path(), false).unwrap();
        assert!(
            dry.qr_legacy_files.iter().any(|f| f == "index.html"),
            "{:?}",
            dry.qr_legacy_files
        );
        assert!(dry.dry_needs_apply());

        let applied = migrate_www_dir(dir.path(), true).unwrap();
        assert!(
            applied.qr_migrated.iter().any(|f| f == "index.html"),
            "{:?}",
            applied.qr_migrated
        );
        let body = fs::read_to_string(dir.path().join("index.html")).unwrap();
        assert!(body.contains("setQrCodeImage"), "{body}");
        assert!(!body.contains("/qr?data="), "{body}");
        assert!(
            body.contains("qrcode.min.js"),
            "script tag injected: {body}"
        );
        assert!(dir.path().join("qrcode.min.js").is_file());
        let main = fs::read_to_string(dir.path().join("main.js")).unwrap();
        assert!(main.contains("function setQrCodeImage"), "{main}");
        assert!(applied.main_js_qr_helper_added || main.contains("setQrCodeImage"));
        assert!(applied.qrcode_js_copied);

        // Idempotent
        let again = migrate_www_dir(dir.path(), true).unwrap();
        assert!(
            again.qr_legacy_files.is_empty() || again.qr_migrated.is_empty(),
            "{again:?}"
        );
        assert!(!again.qrcode_js_copied);
    }

    #[test]
    fn scan_warns_on_obtainium_style_percent_brace() {
        let src = r#"<!DOCTYPE html><a href="https://example.com/x?j={%22a%22:1}">x</a>"#;
        let w = scan_literal_brace_warnings(src, "download.html");
        assert!(!w.is_empty(), "expected warning");
        assert!(w[0].contains("download.html"));
        assert!(w[0].contains("raw"));
    }

    #[test]
    fn scan_ok_for_normal_minijinja() {
        let src = r#"{% if RegistrationOpen %}yes{% endif %} {{ MailDomain }}"#;
        let w = scan_literal_brace_warnings(src, "index.html");
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn format_template_error_mentions_raw_and_migrate() {
        let m = format_www_template_error(
            "download.html",
            "syntax error: unexpected `.` (in download.html:6)",
        );
        assert!(m.contains("download.html"));
        assert!(m.contains("raw"));
        assert!(m.contains("html-migrate"));
    }

    #[test]
    fn migrate_go_and_qr_together() {
        let dir = tempfile::tempdir().unwrap();
        let html = r#"<!DOCTYPE html>
<head><script src="./main.js"></script></head>
<body>{{if .RegistrationOpen}}open{{end}}
<script>
document.getElementById('result-qr').src = `/qr?data=${encodeURIComponent(currentLink)}`;
</script>
</body>"#;
        fs::write(dir.path().join("index.html"), html).unwrap();
        fs::write(dir.path().join("main.js"), "// empty\n").unwrap();
        let r = migrate_www_dir(dir.path(), true).unwrap();
        assert!(r.go_style_files.iter().any(|f| f == "index.html"));
        let body = fs::read_to_string(dir.path().join("index.html")).unwrap();
        assert!(body.contains("{% if RegistrationOpen %}"));
        assert!(body.contains("setQrCodeImage"));
        assert!(!body.contains("/qr?data="));
    }

    /// Prose mentioning `/qr?data=` must not trigger QR migration / script injection.
    #[test]
    fn migrate_ignores_qr_prose_mentions() {
        let dir = tempfile::tempdir().unwrap();
        let html = r#"<!DOCTYPE html><body>
<p>The old /qr?data= endpoint was removed; use client-side QR.</p>
<script src="./main.js"></script>
</body>"#;
        fs::write(dir.path().join("docs.html"), html).unwrap();
        fs::write(dir.path().join("main.js"), "function other() {}\n").unwrap();

        let dry = migrate_www_dir(dir.path(), false).unwrap();
        assert!(
            !dry.dry_needs_apply(),
            "prose-only /qr?data= must not need apply: {:?}",
            dry.qr_legacy_files
        );

        let applied = migrate_www_dir(dir.path(), true).unwrap();
        assert!(applied.migrated.is_empty(), "{applied:?}");
        assert!(applied.qr_migrated.is_empty(), "{applied:?}");
        assert!(!applied.qrcode_js_copied);
        assert!(!applied.main_js_qr_helper_added);
        let body = fs::read_to_string(dir.path().join("docs.html")).unwrap();
        assert!(
            !body.contains("qrcode.min.js"),
            "must not inject qrcode into prose-only page: {body}"
        );
        assert!(!dir.path().join("qrcode.min.js").is_file());
        let main = fs::read_to_string(dir.path().join("main.js")).unwrap();
        assert!(
            !main.contains("setQrCodeImage"),
            "must not append helper: {main}"
        );
    }

    /// Already-modern custom tree (client QR + Minijinja) is a no-op.
    #[test]
    fn migrate_modern_client_qr_tree_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let html = r#"<!DOCTYPE html><head>
<script src="./qrcode.min.js"></script>
<script src="./main.js"></script>
</head><body>
<script>setQrCodeImage(document.getElementById('result-qr'), currentLink);</script>
{% if RegistrationOpen %}open{% endif %}
</body>"#;
        fs::write(dir.path().join("index.html"), html).unwrap();
        fs::write(
            dir.path().join("main.js"),
            "function setQrCodeImage(imgEl, text, cellSize) {}\n",
        )
        .unwrap();
        fs::write(dir.path().join("qrcode.min.js"), "/* stub */\n").unwrap();

        let dry = migrate_www_dir(dir.path(), false).unwrap();
        assert!(!dry.dry_needs_apply(), "{dry:?}");
        let applied = migrate_www_dir(dir.path(), true).unwrap();
        assert!(applied.migrated.is_empty());
        assert!(applied.qr_migrated.is_empty());
        assert!(!applied.qrcode_js_copied);
    }

    #[test]
    fn source_has_legacy_qr_requires_rewriteable_assignment() {
        assert!(!source_has_legacy_qr("see /qr?data= in docs"));
        assert!(source_has_legacy_qr(
            r#"document.getElementById("result-qr").src = `/qr?data=${encodeURIComponent(currentLink)}`;"#
        ));
    }
}
