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

//! `chatmail registration-tokens` — Madmail `ctl/registration_token.go`.

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chatmail_config::cli::RegistrationTokensCommand;
use chatmail_config::Args;
use chatmail_types::{ChatmailError, Result};
use chatmail_db::{db_execute, db_fetch_all, db_fetch_optional, db_fetch_scalar, pg_sql, DbPool};
use getrandom::fill;

use super::context::CtlContext;

type TokenRow = (
    String,
    i32,
    i32,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub async fn registration_tokens(args: &Args, cmd: &RegistrationTokensCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;

    match cmd {
        RegistrationTokensCommand::Create {
            token,
            max_uses,
            comment,
            expires,
        } => create_token(&pool, token.as_deref(), *max_uses, comment, expires.as_deref()).await,
        RegistrationTokensCommand::List => list_tokens(&pool).await,
        RegistrationTokensCommand::Status { token } => status_token(&pool, token).await,
        RegistrationTokensCommand::Delete { token } => delete_token(&pool, token).await,
    }
}

async fn create_token(
    pool: &DbPool,
    token: Option<&str>,
    max_uses: i32,
    comment: &str,
    expires: Option<&str>,
) -> Result<()> {
    let mut token = token.map(str::trim).unwrap_or("").to_string();
    if token.is_empty() {
        token = generate_token_string()?;
    }
    let max_uses = if max_uses <= 0 { 1 } else { max_uses };
    let expires_at = expires
        .map(parse_expires_duration)
        .transpose()?
        .map(format_sqlite_expires);

    db_execute!(
        pool,
        "INSERT INTO registration_tokens (token, max_uses, used_count, comment, expires_at)
         VALUES (?, ?, 0, ?, ?)",
        token.as_str(),
        max_uses,
        comment,
        expires_at.as_deref()
    )?;

    if io::stdout().is_terminal() {
        println!();
        println!("  Token:      {token}");
        println!("  Max Uses:   {max_uses}");
        if !comment.is_empty() {
            println!("  Comment:    {comment}");
        }
        if let Some(ref exp) = expires_at {
            println!("  Expires At: {exp}");
        }
        println!();
    } else {
        print!("{token}");
        io::stdout().flush().ok();
    }
    Ok(())
}

async fn list_tokens(pool: &DbPool) -> Result<()> {
    let rows: Vec<TokenRow> = db_fetch_all!(
        pool,
        TokenRow,
        "SELECT token, max_uses, used_count, comment, expires_at, created_at
         FROM registration_tokens ORDER BY created_at DESC"
    )?;

    if rows.is_empty() {
        println!("No registration tokens found.");
        return Ok(());
    }

    let now = SystemTime::now();
    println!();
    println!(
        "{:<28} {:<8} {:<10} {:<10} {:<10} {}",
        "TOKEN", "MAX", "CONSUMED", "PENDING", "STATUS", "COMMENT"
    );
    println!("{}", "-".repeat(90));

    for (token, max_uses, used_count, comment, expires_at, _created_at) in rows {
        let pending: i64 = db_fetch_scalar!(
            pool,
            i64,
            "SELECT COUNT(*) FROM quotas WHERE used_token = ? AND first_login_at = 1",
            token.as_str()
        )?;
        let status = token_status(max_uses, used_count, pending as i32, expires_at.as_deref(), now);
        let comment = comment.unwrap_or_default();
        let comment = truncate_str(&comment, 20);
        println!(
            "{:<28} {:<8} {:<10} {:<10} {:<10} {}",
            truncate_str(&token, 28),
            max_uses,
            used_count,
            pending,
            status,
            comment
        );
    }
    println!();
    Ok(())
}

async fn status_token(pool: &DbPool, token: &str) -> Result<()> {
    let t: TokenRow = db_fetch_optional!(
        pool,
        TokenRow,
        "SELECT token, max_uses, used_count, comment, expires_at, created_at
         FROM registration_tokens WHERE token = ?",
        token
    )?
    .ok_or_else(|| ChatmailError::config(format!("token not found: {token}")))?;

    let (ref token_s, max_uses, used_count, ref comment, ref expires_at, ref created_at) = t;

    let pending: i64 = db_fetch_scalar!(
        pool,
        i64,
        "SELECT COUNT(*) FROM quotas WHERE used_token = ? AND first_login_at = 1",
        token_s.as_str()
    )?;

    let now = SystemTime::now();
    let status = token_status(max_uses, used_count, pending as i32, expires_at.as_deref(), now);
    let available = i64::from(max_uses) - i64::from(used_count) - pending;

    println!();
    println!("  Token:      {token_s}");
    println!("  Status:     {status}");
    println!("  Max Uses:   {max_uses}");
    println!("  Consumed:   {used_count} (confirmed first logins)");
    println!("  Pending:    {pending} (reserved, awaiting first login)");
    println!("  Available:  {available}");
    if let Some(ref c) = comment {
        if !c.is_empty() {
            println!("  Comment:    {c}");
        }
    }
    if let Some(ref created) = created_at {
        println!("  Created At: {created}");
    }
    if let Some(ref exp) = expires_at {
        println!("  Expires At: {exp}");
        if let Some(exp_t) = parse_sqlite_timestamp(exp) {
            if exp_t > now {
                let left = exp_t.duration_since(now).unwrap_or_default();
                println!("  Expires In: {}m", left.as_secs() / 60);
            } else {
                let ago = now.duration_since(exp_t).unwrap_or_default();
                println!("  Expired:    {}m ago", ago.as_secs() / 60);
            }
        }
    }

    let quotas: Vec<(String, i64)> = db_fetch_all!(
        pool,
        (String, i64),
        "SELECT username, first_login_at FROM quotas WHERE used_token = ?",
        token_s.as_str()
    )?;

    if !quotas.is_empty() {
        println!("\n  Pending Accounts ({}):", quotas.len());
        for (username, first_login_at) in quotas {
            let login_status = if first_login_at > 1 {
                "consumed"
            } else {
                "awaiting first login"
            };
            println!("    - {username} ({login_status})");
        }
    }
    println!();
    Ok(())
}

async fn delete_token(pool: &DbPool, token: &str) -> Result<()> {
    let affected = match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM registration_tokens WHERE token = ?")
                .bind(token)
                .execute(p)
                .await?
                .rows_affected()
        }
        DbPool::Postgres(p) => {
            sqlx::query(&pg_sql("DELETE FROM registration_tokens WHERE token = ?"))
                .bind(token)
                .execute(p)
                .await?
                .rows_affected()
        }
    };
    if affected == 0 {
        return Err(ChatmailError::config(format!("token not found: {token}")));
    }
    println!("Deleted token: {token}");
    Ok(())
}

fn token_status(
    max_uses: i32,
    used_count: i32,
    pending: i32,
    expires_at: Option<&str>,
    now: SystemTime,
) -> &'static str {
    if let Some(exp) = expires_at {
        if let Some(exp_t) = parse_sqlite_timestamp(exp) {
            if now > exp_t {
                return "expired";
            }
        }
    }
    if i64::from(used_count) + i64::from(pending) >= i64::from(max_uses) {
        return "exhausted";
    }
    "active"
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else if max <= 3 {
        s.chars().take(max).collect()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

fn generate_token_string() -> Result<String> {
    let mut b = [0u8; 18];
    fill(&mut b).map_err(|e| ChatmailError::config(format!("failed to generate token: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(b))
}

fn parse_expires_duration(s: &str) -> std::result::Result<Duration, ChatmailError> {
    let s = s.trim();
    if let Some(h) = s.strip_suffix('h') {
        let n: u64 = h
            .trim()
            .parse()
            .map_err(|e| ChatmailError::config(format!("invalid expiration duration: {e}")))?;
        return Ok(Duration::from_secs(n * 3600));
    }
    if let Some(m) = s.strip_suffix('m') {
        let n: u64 = m
            .trim()
            .parse()
            .map_err(|e| ChatmailError::config(format!("invalid expiration duration: {e}")))?;
        return Ok(Duration::from_secs(n * 60));
    }
    Err(ChatmailError::config(format!(
        "invalid expiration duration: {s} (use e.g. 72h)"
    )))
}

fn format_sqlite_expires(d: Duration) -> String {
    let t = SystemTime::now() + d;
    format_system_time_rfc3339(t)
}

fn format_system_time_rfc3339(t: SystemTime) -> String {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    time::OffsetDateTime::from_unix_timestamp(secs)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn parse_sqlite_timestamp(s: &str) -> Option<SystemTime> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
    {
        return Some(
            UNIX_EPOCH
                + Duration::from_secs(dt.unix_timestamp().max(0) as u64)
                + Duration::from_nanos(dt.nanosecond() as u64),
        );
    }
    let normalized = s.replace(' ', "T");
    let with_z = if normalized.ends_with('Z') {
        normalized
    } else {
        format!("{normalized}Z")
    };
    time::OffsetDateTime::parse(&with_z, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|dt| {
            UNIX_EPOCH
                + Duration::from_secs(dt.unix_timestamp().max(0) as u64)
                + Duration::from_nanos(dt.nanosecond() as u64)
        })
}
