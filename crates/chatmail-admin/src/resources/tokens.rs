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

//! `/admin/registration-token` — Madmail `resources.TokensHandler`.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chatmail_db::{db_execute, db_fetch_all, db_fetch_optional, db_fetch_scalar, pg_sql, schema::quota_table, DbPool};
use getrandom::getrandom;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

#[derive(Deserialize)]
struct TokenCreateRequest {
    #[serde(default)]
    token: String,
    #[serde(default)]
    max_uses: i32,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    expires_in: String,
    expires_at: Option<String>,
}

#[derive(Deserialize)]
struct TokenDeleteRequest {
    token: String,
}

type TokenRow = (
    String,
    i32,
    i32,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub async fn registration_token(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => list_tokens(st).await,
        "POST" => create_or_update_token(st, body).await,
        "DELETE" => delete_token(st, body).await,
        _ => Err((
            405,
            format!("method {method} not allowed for /admin/registration-token"),
        )),
    }
}

async fn list_tokens(st: &AdminState) -> AdminResult {
    let rows: Vec<TokenRow> = db_fetch_all!(
        &st.pool,
        TokenRow,
        "SELECT token, max_uses, used_count, comment, expires_at, created_at
         FROM registration_tokens ORDER BY created_at DESC"
    )
    .map_err(db_err)?;

    let pending_rows: Vec<(String, i64)> = {
        let qt = quota_table(&st.pool).await.map_err(db_err)?;
        let sql = format!(
            "SELECT used_token, COUNT(*) AS cnt FROM {qt}
         WHERE used_token != '' AND first_login_at = 1
         GROUP BY used_token"
        );
        db_fetch_all!(&st.pool, (String, i64), &sql).map_err(db_err)?
    };

    let pending: HashMap<String, i64> = pending_rows.into_iter().collect();
    let now = SystemTime::now();

    let tokens: Vec<Value> = rows
        .into_iter()
        .map(|(token, max_uses, used_count, comment, expires_at, created_at)| {
            let pending_reservations = pending.get(&token).copied().unwrap_or(0) as i32;
            let status = token_status(max_uses, used_count, pending_reservations, expires_at.as_deref(), now);
            json!({
                "token": token,
                "max_uses": max_uses,
                "used_count": used_count,
                "pending_reservations": pending_reservations,
                "comment": comment.unwrap_or_default(),
                "created_at": created_at.unwrap_or_default(),
                "expires_at": expires_at,
                "status": status,
            })
        })
        .collect();

    Ok((
        200,
        Some(json!({ "tokens": tokens, "total": tokens.len() })),
    ))
}

fn token_status(
    max_uses: i32,
    used_count: i32,
    pending: i32,
    expires_at: Option<&str>,
    now: SystemTime,
) -> &'static str {
    if let Some(exp) = expires_at {
        if is_expired(exp, now) {
            return "expired";
        }
    }
    if used_count >= max_uses {
        return "exhausted";
    }
    if i64::from(used_count) + i64::from(pending) >= i64::from(max_uses) {
        return "exhausted";
    }
    "active"
}

fn is_expired(expires_at: &str, now: SystemTime) -> bool {
    parse_sqlite_timestamp(expires_at)
        .map(|exp| now > exp)
        .unwrap_or(false)
}

fn parse_sqlite_timestamp(s: &str) -> Option<SystemTime> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339) {
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

async fn create_or_update_token(st: &AdminState, body: &Value) -> AdminResult {
    let req: TokenCreateRequest = serde_json::from_value(body.clone())
        .map_err(|e| (400, e.to_string()))?;

    let mut token = req.token.trim().to_string();
    if token.is_empty() {
        token = generate_token_string().map_err(|e| (500, e))?;
    }

    let max_uses = if req.max_uses <= 0 { 1 } else { req.max_uses };
    let expires_at = resolve_expires_at(&req).map_err(|e| (400, e))?;

    let existing: Option<TokenRow> = db_fetch_optional!(
        &st.pool,
        TokenRow,
        "SELECT token, max_uses, used_count, comment, expires_at, created_at
         FROM registration_tokens WHERE token = ?",
        token.as_str()
    )
    .map_err(db_err)?;

    if let Some((_, _, used_count, _, _, created_at)) = existing {
        db_execute!(
            &st.pool,
            "UPDATE registration_tokens SET max_uses = ?, comment = ?, expires_at = ? WHERE token = ?",
            max_uses,
            req.comment.as_str(),
            expires_at.as_deref(),
            token.as_str()
        )
        .map_err(db_err)?;

        let pending = pending_for_token(&st.pool, &token).await?;
        let body = token_json(
            &token,
            max_uses,
            used_count,
            pending,
            &req.comment,
            created_at.as_deref(),
            expires_at.as_deref(),
            "active",
        );
        return Ok((200, Some(body)));
    }

    db_execute!(
        &st.pool,
        "INSERT INTO registration_tokens (token, max_uses, used_count, comment, expires_at)
         VALUES (?, ?, 0, ?, ?)",
        token.as_str(),
        max_uses,
        req.comment.as_str(),
        expires_at.as_deref()
    )
    .map_err(db_err)?;

    let created_at: Option<String> = db_fetch_scalar!(
        &st.pool,
        String,
        "SELECT created_at FROM registration_tokens WHERE token = ?",
        token.as_str()
    )
    .ok();

    let body = token_json(
        &token,
        max_uses,
        0,
        0,
        &req.comment,
        created_at.as_deref(),
        expires_at.as_deref(),
        "active",
    );
    Ok((201, Some(body)))
}

async fn delete_token(st: &AdminState, body: &Value) -> AdminResult {
    let req: TokenDeleteRequest = serde_json::from_value(body.clone())
        .map_err(|e| (400, e.to_string()))?;
    if req.token.is_empty() {
        return Err((400, "token is required".into()));
    }

    let affected = match &st.pool {
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM registration_tokens WHERE token = ?")
                .bind(&req.token)
                .execute(p)
                .await
                .map_err(db_err)?
                .rows_affected()
        }
        DbPool::Postgres(p) => {
            sqlx::query(&pg_sql("DELETE FROM registration_tokens WHERE token = ?"))
                .bind(&req.token)
                .execute(p)
                .await
                .map_err(db_err)?
                .rows_affected()
        }
    };

    if affected == 0 {
        return Err((404, "token not found".into()));
    }

    Ok((200, Some(json!({ "deleted": req.token }))))
}

async fn pending_for_token(pool: &DbPool, token: &str) -> Result<i32, (u16, String)> {
    let qt = quota_table(pool).await.map_err(db_err)?;
    let sql = format!(
        "SELECT COUNT(*) FROM {qt} WHERE used_token = ? AND first_login_at = 1"
    );
    let n: i64 = db_fetch_scalar!(pool, i64, &sql, token).map_err(db_err)?;
    Ok(n as i32)
}

fn token_json(
    token: &str,
    max_uses: i32,
    used_count: i32,
    pending: i32,
    comment: &str,
    created_at: Option<&str>,
    expires_at: Option<&str>,
    status: &str,
) -> Value {
    json!({
        "token": token,
        "max_uses": max_uses,
        "used_count": used_count,
        "pending_reservations": pending,
        "comment": comment,
        "created_at": created_at.unwrap_or(""),
        "expires_at": expires_at,
        "status": status,
    })
}

fn resolve_expires_at(req: &TokenCreateRequest) -> Result<Option<String>, String> {
    if let Some(ref at) = req.expires_at {
        if at.trim().is_empty() {
            return Ok(None);
        }
        return Ok(Some(at.trim().to_string()));
    }
    if req.expires_in.trim().is_empty() {
        return Ok(None);
    }
    let dur = parse_duration(&req.expires_in)?;
    let exp = SystemTime::now() + dur;
    Ok(Some(format_system_time_sqlite(exp)))
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if let Some(h) = s.strip_suffix('h') {
        let n: u64 = h
            .trim()
            .parse()
            .map_err(|e| format!("invalid expires_in duration: {e}"))?;
        return Ok(Duration::from_secs(n * 3600));
    }
    if let Some(m) = s.strip_suffix('m') {
        let n: u64 = m
            .trim()
            .parse()
            .map_err(|e| format!("invalid expires_in duration: {e}"))?;
        return Ok(Duration::from_secs(n * 60));
    }
    if let Some(sec) = s.strip_suffix('s') {
        let n: u64 = sec
            .trim()
            .parse()
            .map_err(|e| format!("invalid expires_in duration: {e}"))?;
        return Ok(Duration::from_secs(n));
    }
    let n: u64 = s
        .parse()
        .map_err(|e| format!("invalid expires_in duration: {e}"))?;
    Ok(Duration::from_secs(n))
}

fn format_system_time_sqlite(t: SystemTime) -> String {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let nanos = d.subsec_nanos();
    let dt = time::OffsetDateTime::from_unix_timestamp(secs)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    let dt = dt + time::Duration::nanoseconds(nanos as i64);
    dt.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn generate_token_string() -> Result<String, String> {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut b = [0u8; 24];
    getrandom(&mut b).map_err(|e| format!("failed to generate token: {e}"))?;
    Ok(b
        .iter()
        .map(|x| CHARSET[(*x as usize) % CHARSET.len()] as char)
        .collect())
}
