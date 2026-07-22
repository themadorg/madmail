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

//! `chatmail html-export` / `html-serve` / `html-migrate` (Madmail `ctl/html.go`).

use std::io::IsTerminal;
use std::path::Path;

use chatmail_config::{load_config, update_config_www_dir, Args};
use chatmail_types::{ChatmailError, Result};
use chatmail_www::{export_www_files, migrate_www_dir};

use super::context::CtlContext;
use super::output::CtlOut;
use super::util::confirm;

pub async fn html_export(args: &Args, dest: &Path) -> Result<()> {
    let _ = CtlContext::from_args(args)?;
    let out = CtlOut::from_args(args, "html-export");
    let n = export_www_files(dest)?;
    out.done_msg(
        format!("Successfully exported {n} files to {}", dest.display()),
        serde_json::json!({ "dest": dest.display().to_string(), "files": n }),
        format!("Exported {n} files"),
    )
}

pub async fn html_serve(args: &Args, www_dir: &str) -> Result<()> {
    let _ctx = CtlContext::from_args(args)?;
    let out = CtlOut::from_args(args, "html-serve");

    let embedded = matches!(
        www_dir.trim().to_ascii_lowercase().as_str(),
        "embedded" | "embed" | "internal"
    );

    let www_path = if embedded {
        None
    } else {
        let p = Path::new(www_dir);
        if !p.is_dir() {
            return Err(ChatmailError::config(format!(
                "directory not found: {}",
                p.display()
            )));
        }
        Some(p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
    };

    if !args.config.is_file() {
        return Err(ChatmailError::config(format!(
            "config file not found: {} — pass --config",
            args.config.display()
        )));
    }

    update_config_www_dir(&args.config, www_path.as_deref())?;

    if out.is_json() {
        return out.done_msg(
            "",
            serde_json::json!({
                "config": args.config.display().to_string(),
                "embedded": embedded,
                "www_dir": www_path.as_ref().map(|p| p.display().to_string()),
            }),
            if embedded {
                "Updated config to use embedded HTML".into()
            } else {
                format!(
                    "Updated config to serve HTML from {}",
                    www_path.as_ref().unwrap().display()
                )
            },
        );
    }

    if embedded {
        out.line(format!(
            "Successfully updated {} to use embedded HTML files.",
            args.config.display()
        ));
    } else {
        let p = www_path.as_ref().unwrap();
        out.line(format!(
            "Successfully updated {} to serve HTML from {}",
            args.config.display(),
            p.display()
        ));
        out.blank();
        out.line("Ensure the chatmail service user can read this directory.");
        out.line(format!(
            "Example: sudo chown -R chatmail:chatmail {}",
            p.display()
        ));
    }
    out.blank();
    out.line("Restart chatmail to apply: sudo systemctl restart madmail");
    Ok(())
}

/// Convert custom `www_dir` Go templates to Minijinja and fix legacy QR/static assets.
///
/// Used interactively by operators and non-interactively from `madmail update`
/// (re-exec of the new binary after replace).
pub async fn html_migrate(args: &Args, yes: bool) -> Result<()> {
    let _ = CtlContext::from_args(args)?;
    let out = CtlOut::from_args(args, "html-migrate");

    if !args.config.is_file() {
        return Err(ChatmailError::config(format!(
            "config file not found: {} — pass --config",
            args.config.display()
        )));
    }

    let cfg = load_config(&args.config)?;
    let Some(www_dir) = cfg.www_dir.as_ref() else {
        let msg = "No custom www_dir in config (embedded site); nothing to migrate.";
        return out.done_msg(
            msg,
            serde_json::json!({
                "config": args.config.display().to_string(),
                "www_dir": null,
                "action": "noop_embedded",
            }),
            msg,
        );
    };

    if !www_dir.is_dir() {
        return Err(ChatmailError::config(format!(
            "www_dir is set but not a directory: {}",
            www_dir.display()
        )));
    }

    let dry = migrate_www_dir(www_dir, false)?;
    print_literal_brace_warnings(&out, &dry.literal_brace_warnings);

    if !dry.dry_needs_apply() {
        let msg = format!(
            "Custom www_dir {} has no Go-style HTML templates and no legacy /qr?data= QR usage \
             (already Minijinja / client-side QR, or no templates).",
            www_dir.display()
        );
        if !out.is_json() && !dry.literal_brace_warnings.is_empty() {
            out.blank();
            out.line(
                "Literal `{%` / `{{` warnings above are not auto-fixed — wrap with \
                 `{% raw %}…{% endraw %}` if they are plain text or URLs.",
            );
        }
        return out.done_msg(
            &msg,
            serde_json::json!({
                "config": args.config.display().to_string(),
                "www_dir": www_dir.display().to_string(),
                "scanned": dry.scanned,
                "go_style_files": dry.go_style_files,
                "qr_legacy_files": dry.qr_legacy_files,
                "literal_brace_warnings": dry.literal_brace_warnings,
                "action": "noop_already_migrated",
            }),
            "No Go-style templates or legacy QR found",
        );
    }

    if !out.is_json() {
        if !dry.go_style_files.is_empty() {
            out.line(format!(
                "Found {} Go-style HTML template(s) under {}:",
                dry.go_style_files.len(),
                www_dir.display()
            ));
            for (i, f) in dry.go_style_files.iter().enumerate() {
                if i >= 12 {
                    out.line(format!(
                        "  … and {} more",
                        dry.go_style_files.len().saturating_sub(12)
                    ));
                    break;
                }
                out.line(format!("  - {f}"));
            }
            out.blank();
        }
        if !dry.qr_legacy_files.is_empty() {
            out.line(format!(
                "Found {} legacy QR / client-QR asset issue(s) under {}:",
                dry.qr_legacy_files.len(),
                www_dir.display()
            ));
            for (i, f) in dry.qr_legacy_files.iter().enumerate() {
                if i >= 12 {
                    out.line(format!(
                        "  … and {} more",
                        dry.qr_legacy_files.len().saturating_sub(12)
                    ));
                    break;
                }
                out.line(format!("  - {f}"));
            }
            out.blank();
        }
        out.line(
            "This rewrites Go html/template → Minijinja and/or `/qr?data=` → client-side QR \
             (setQrCodeImage + qrcode.min.js). Backups: *.go-template.bak / main.js.qr-compat.bak.",
        );
    }

    let interactive = std::io::stdin().is_terminal() && !out.is_json();
    let should_apply = if yes {
        true
    } else if interactive {
        confirm(
            "Migrate custom www (Go templates → Minijinja and/or legacy QR → client-side)?",
            false,
        )?
    } else {
        if !out.is_json() {
            out.line(
                "Non-interactive session: not converting. Re-run with: \
                 madmail html-migrate --yes",
            );
        }
        return out.done_msg(
            "Skipped (non-interactive; pass --yes to convert)",
            serde_json::json!({
                "config": args.config.display().to_string(),
                "www_dir": www_dir.display().to_string(),
                "scanned": dry.scanned,
                "go_style_files": dry.go_style_files,
                "qr_legacy_files": dry.qr_legacy_files,
                "literal_brace_warnings": dry.literal_brace_warnings,
                "action": "skipped_noninteractive",
            }),
            "Skipped non-interactive",
        );
    };

    if !should_apply {
        return out.done_msg(
            "Left custom www templates and QR assets unchanged.",
            serde_json::json!({
                "config": args.config.display().to_string(),
                "www_dir": www_dir.display().to_string(),
                "scanned": dry.scanned,
                "go_style_files": dry.go_style_files,
                "qr_legacy_files": dry.qr_legacy_files,
                "literal_brace_warnings": dry.literal_brace_warnings,
                "action": "declined",
            }),
            "Declined",
        );
    }

    let report = migrate_www_dir(www_dir, true)?;
    if !out.is_json() {
        if !report.migrated.is_empty() || !report.qr_migrated.is_empty() {
            out.line(format!(
                "Migrated {} HTML file(s); QR/static updates: {}; backups: {}",
                report.migrated.len(),
                report.qr_migrated.len(),
                report.backups.len()
            ));
            for f in &report.migrated {
                out.line(format!("  ✓ {f}"));
            }
            for f in &report.qr_migrated {
                if !report.migrated.contains(f) {
                    out.line(format!("  ✓ QR/static: {f}"));
                }
            }
            if report.qrcode_js_copied {
                out.line("  ✓ copied qrcode.min.js from embedded assets");
            }
            if report.main_js_qr_helper_added {
                out.line("  ✓ appended setQrCodeImage to main.js");
            }
        }
        if !report.errors.is_empty() {
            for e in &report.errors {
                out.line(format!("  ⚠ {e}"));
            }
        }
        print_literal_brace_warnings(&out, &report.literal_brace_warnings);
        if !report.literal_brace_warnings.is_empty() {
            out.blank();
            out.line(
                "Literal `{%` / `{{` warnings are not auto-fixed — wrap with \
                 `{% raw %}…{% endraw %}` if they are plain text or URLs.",
            );
        }
    }

    let action = if report.migrated.is_empty()
        && report.qr_migrated.is_empty()
        && !report.qrcode_js_copied
        && !report.main_js_qr_helper_added
    {
        "noop_already_migrated"
    } else {
        "migrated"
    };

    out.done_msg(
        format!(
            "Migrated {} template(s), {} QR/static update(s) under {}",
            report.migrated.len(),
            report.qr_migrated.len(),
            www_dir.display()
        ),
        serde_json::json!({
            "config": args.config.display().to_string(),
            "www_dir": www_dir.display().to_string(),
            "scanned": report.scanned,
            "go_style_files": report.go_style_files,
            "migrated": report.migrated,
            "qr_legacy_files": report.qr_legacy_files,
            "qr_migrated": report.qr_migrated,
            "qrcode_js_copied": report.qrcode_js_copied,
            "main_js_qr_helper_added": report.main_js_qr_helper_added,
            "literal_brace_warnings": report.literal_brace_warnings,
            "backups": report.backups,
            "errors": report.errors,
            "action": action,
        }),
        format!(
            "Migrated {} file(s), {} QR/static",
            report.migrated.len(),
            report.qr_migrated.len()
        ),
    )
}

fn print_literal_brace_warnings(out: &CtlOut, warnings: &[String]) {
    if out.is_json() || warnings.is_empty() {
        return;
    }
    out.blank();
    out.line(format!(
        "⚠ {} possible literal `{{%` / `{{{{` sequence(s) (Minijinja may 500):",
        warnings.len()
    ));
    for (i, w) in warnings.iter().enumerate() {
        if i >= 8 {
            out.line(format!("  … and {} more", warnings.len().saturating_sub(8)));
            break;
        }
        out.line(format!("  - {w}"));
    }
}
