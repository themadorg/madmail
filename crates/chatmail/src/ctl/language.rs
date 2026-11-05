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

//! `chatmail language` — website language (`__LANGUAGE__`).

use chatmail_config::cli::LanguageCommand;
use chatmail_config::Args;
use chatmail_db::{delete_setting, get_setting, set_setting, settings_keys};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;
use super::output::CtlOut;

const VALID: &[(&str, &str)] = &[
    ("en", "English"),
    ("fa", "فارسی (Farsi)"),
    ("ru", "Русский (Russian)"),
    ("es", "Español (Spanish)"),
];

/// Normalize and validate a language code (`en`, `fa`, `ru`, `es`).
pub fn validate_language_code(lang: &str) -> Result<String> {
    let code = lang.trim().to_lowercase();
    if language_name(&code).is_empty() {
        return Err(ChatmailError::config(format!(
            "unsupported language: {lang}\nSupported: en, fa, ru, es"
        )));
    }
    Ok(code)
}

pub async fn language(args: &Args, cmd: Option<&LanguageCommand>) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        None | Some(LanguageCommand::Status) => status(args, &ctx, &pool).await,
        Some(LanguageCommand::Set { lang }) => set_lang(args, &pool, lang).await,
        Some(LanguageCommand::Reset) => reset(args, &pool).await,
    }
}

async fn status(args: &Args, ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<()> {
    let out = CtlOut::from_args(args, "language status");
    let db_lang = get_setting(pool, settings_keys::LANGUAGE).await?;
    let config_default = ctx.config.language.as_deref().unwrap_or("en").to_string();

    let protected = pool.protection_source(settings_keys::LANGUAGE);
    if out.is_json() {
        let (current, source) = match db_lang.as_deref() {
            Some(v) if !v.is_empty() && protected.is_some() => (v.to_string(), "protected"),
            Some(v) if !v.is_empty() => (v.to_string(), "db"),
            _ => (config_default.clone(), "config"),
        };
        return out.emit(serde_json::json!({
            "current": current,
            "config_default": config_default,
            "source": source,
        }));
    }

    let display = if let Some(v) = db_lang {
        if !v.is_empty() {
            match &protected {
                Some(source) => format!("{} — {} (protected by {source})", v, language_name(&v)),
                None => format!("{} — {} (DB override)", v, language_name(&v)),
            }
        } else {
            "(config default)".into()
        }
    } else {
        "(config default)".into()
    };

    let config_display = format!("{} — {}", config_default, language_name(&config_default));

    out.blank();
    out.line(format!("  Website language:  {display}"));
    if display.contains("config default") {
        out.line(format!("  Config default:    {config_display}"));
    }
    out.blank();
    Ok(())
}

async fn set_lang(args: &Args, pool: &chatmail_db::DbPool, lang: &str) -> Result<()> {
    let out = CtlOut::from_args(args, "language set");
    let lang = validate_language_code(lang)?;
    set_setting(pool, settings_keys::LANGUAGE, &lang).await?;
    out.done_msg(
        format!(
            "🌐 Website language set to {lang} — {} (effective immediately)",
            language_name(&lang)
        ),
        serde_json::json!({ "language": lang }),
        format!("Website language set to {lang}"),
    )
}

async fn reset(args: &Args, pool: &chatmail_db::DbPool) -> Result<()> {
    let out = CtlOut::from_args(args, "language reset");
    delete_setting(pool, settings_keys::LANGUAGE).await?;
    out.done_msg(
        "🔄 Website language reset to config default (effective immediately)",
        serde_json::json!({ "reset": true }),
        "Website language reset to config default",
    )
}

fn language_name(code: &str) -> &'static str {
    VALID
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| *n)
        .unwrap_or("")
}
