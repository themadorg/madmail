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
        None | Some(LanguageCommand::Status) => status(&ctx, &pool).await,
        Some(LanguageCommand::Set { lang }) => set_lang(&pool, lang).await,
        Some(LanguageCommand::Reset) => reset(&pool).await,
    }
}

async fn status(ctx: &CtlContext, pool: &chatmail_db::DbPool) -> Result<()> {
    let display = if let Some(v) = get_setting(pool, settings_keys::LANGUAGE).await? {
        if !v.is_empty() {
            format!("{} — {} (DB override)", v, language_name(&v))
        } else {
            "(config default)".into()
        }
    } else {
        "(config default)".into()
    };

    let config_default = ctx
        .config
        .language
        .as_deref()
        .map(|l| format!("{l} — {}", language_name(l)))
        .unwrap_or_else(|| "en".into());

    println!();
    println!("  Website language:  {display}");
    if display.contains("config default") {
        println!("  Config default:    {config_default}");
    }
    println!();
    Ok(())
}

async fn set_lang(pool: &chatmail_db::DbPool, lang: &str) -> Result<()> {
    let lang = validate_language_code(lang)?;
    set_setting(pool, settings_keys::LANGUAGE, &lang).await?;
    println!(
        "🌐 Website language set to {lang} — {} (effective immediately)",
        language_name(&lang)
    );
    Ok(())
}

async fn reset(pool: &chatmail_db::DbPool) -> Result<()> {
    delete_setting(pool, settings_keys::LANGUAGE).await?;
    println!("🔄 Website language reset to config default (effective immediately)");
    Ok(())
}

fn language_name(code: &str) -> &'static str {
    VALID
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| *n)
        .unwrap_or("")
}
