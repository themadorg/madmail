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

use chatmail_db::{
    db_execute, db_fetch_all, db_fetch_optional, db_fetch_scalar, pg_sql, schema::quota_table,
    DbPool,
};
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
        "SELECT token, max_uses, used_count, comment, CAST(expires_at AS TEXT), CAST(created_at AS TEXT)
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
        .map(
            |(token, max_uses, used_count, comment, expires_at, created_at)| {
                let pending_reservations = pending.get(&token).copied().unwrap_or(0) as i32;
                let status = token_status(
                    max_uses,
                    used_count,
                    pending_reservations,
                    expires_at.as_deref(),
                    now,
                );
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
            },
        )
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
    let req: TokenCreateRequest =
        serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;

    let mut token = req.token.trim().to_string();
    if token.is_empty() {
        token = generate_token_string().map_err(|e| (500, e))?;
    }

    let max_uses = if req.max_uses <= 0 { 1 } else { req.max_uses };
    let expires_at = resolve_expires_at(&req).map_err(|e| (400, e))?;

    let existing: Option<TokenRow> = db_fetch_optional!(
        &st.pool,
        TokenRow,
        "SELECT token, max_uses, used_count, comment, CAST(expires_at AS TEXT), CAST(created_at AS TEXT)
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
        "SELECT CAST(created_at AS TEXT) FROM registration_tokens WHERE token = ?",
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
    let req: TokenDeleteRequest =
        serde_json::from_value(body.clone()).map_err(|e| (400, e.to_string()))?;
    if req.token.is_empty() {
        return Err((400, "token is required".into()));
    }

    let affected = match &st.pool {
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM registration_tokens WHERE token = ?")
            .bind(&req.token)
            .execute(p)
            .await
            .map_err(db_err)?
            .rows_affected(),
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
    let sql = format!("SELECT COUNT(*) FROM {qt} WHERE used_token = ? AND first_login_at = 1");
    let n: i64 = db_fetch_scalar!(pool, i64, &sql, token).map_err(db_err)?;
    Ok(n as i32)
}

#[allow(clippy::too_many_arguments)]
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
    let dt =
        time::OffsetDateTime::from_unix_timestamp(secs).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    let dt = dt + time::Duration::nanoseconds(nanos as i64);
    dt.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn generate_token_string() -> Result<String, String> {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut b = [0u8; 24];
    getrandom(&mut b).map_err(|e| format!("failed to generate token: {e}"))?;
    Ok(b.iter()
        .map(|x| CHARSET[(*x as usize) % CHARSET.len()] as char)
        .collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, UNIX_EPOCH};

    use chatmail_config::AppConfig;
    use chatmail_db::{
        attach_registration_token, ensure_new_account_quota, init_memory_db, seed_install_defaults,
    };
    use chatmail_state::AppState;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::AdminState;

    fn fixed_now(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    async fn test_admin_state() -> (AdminState, TempDir) {
        let pool = init_memory_db().await.unwrap();
        seed_install_defaults(&pool).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(AppState::with_quota_and_message_limit(
            dir.path(),
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &AppConfig::default(),
            pool.clone(),
        ));
        app.hydrate(&pool, &AppConfig::default()).await.unwrap();
        let st = AdminState::new(
            pool,
            app,
            AppConfig::default(),
            dir.path().to_path_buf(),
            "example.org".into(),
            "secret-token-01234567890123456789012345678901".into(),
            None,
        );
        (st, dir)
    }

    #[test]
    fn token_status_active_when_uses_remain() {
        let now = fixed_now(1_700_000_000);
        assert_eq!(token_status(5, 2, 1, None, now), "active");
    }

    #[test]
    fn token_status_exhausted_when_used_count_reaches_max() {
        let now = fixed_now(1_700_000_000);
        assert_eq!(token_status(3, 3, 0, None, now), "exhausted");
    }

    #[test]
    fn token_status_exhausted_when_used_plus_pending_reaches_max() {
        let now = fixed_now(1_700_000_000);
        assert_eq!(token_status(2, 1, 1, None, now), "exhausted");
    }

    #[test]
    fn token_status_expired_takes_precedence() {
        let now = fixed_now(1_700_000_000);
        assert_eq!(
            token_status(10, 0, 0, Some("1970-01-01T00:00:00Z"), now),
            "expired"
        );
    }

    #[test]
    fn parse_sqlite_timestamp_accepts_rfc3339_and_sqlite_format() {
        assert!(parse_sqlite_timestamp("2020-01-01T00:00:00Z").is_some());
        assert!(parse_sqlite_timestamp("2020-01-01 00:00:00").is_some());
        assert!(parse_sqlite_timestamp("").is_none());
    }

    #[test]
    fn parse_duration_units_and_bare_seconds() {
        assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
        assert_eq!(parse_duration("30m").unwrap(), Duration::from_secs(1800));
        assert_eq!(parse_duration("90s").unwrap(), Duration::from_secs(90));
        assert_eq!(parse_duration("45").unwrap(), Duration::from_secs(45));
        assert!(parse_duration("bad").is_err());
    }

    #[test]
    fn resolve_expires_at_prefers_explicit_expires_at() {
        let req = TokenCreateRequest {
            token: String::new(),
            max_uses: 0,
            comment: String::new(),
            expires_in: "1h".into(),
            expires_at: Some(" 2030-06-01T12:00:00Z ".into()),
        };
        assert_eq!(
            resolve_expires_at(&req).unwrap().as_deref(),
            Some("2030-06-01T12:00:00Z")
        );
    }

    #[test]
    fn resolve_expires_at_blank_expires_at_is_none() {
        let req = TokenCreateRequest {
            token: String::new(),
            max_uses: 0,
            comment: String::new(),
            expires_in: String::new(),
            expires_at: Some("   ".into()),
        };
        assert!(resolve_expires_at(&req).unwrap().is_none());
    }

    #[test]
    fn resolve_expires_at_from_expires_in() {
        let req = TokenCreateRequest {
            token: String::new(),
            max_uses: 0,
            comment: String::new(),
            expires_in: "3600".into(),
            expires_at: None,
        };
        let exp = resolve_expires_at(&req).unwrap().expect("expires_at set");
        assert!(parse_sqlite_timestamp(&exp).is_some());
    }

    #[test]
    fn generate_token_string_length_and_charset() {
        let token = generate_token_string().unwrap();
        assert_eq!(token.len(), 24);
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn token_json_shape() {
        let v = token_json(
            "abc",
            3,
            1,
            2,
            "note",
            Some("2020-01-01T00:00:00Z"),
            Some("2030-01-01T00:00:00Z"),
            "active",
        );
        assert_eq!(v["token"], "abc");
        assert_eq!(v["max_uses"], 3);
        assert_eq!(v["used_count"], 1);
        assert_eq!(v["pending_reservations"], 2);
        assert_eq!(v["comment"], "note");
        assert_eq!(v["status"], "active");
    }

    #[tokio::test]
    async fn list_tokens_empty() {
        let (st, _dir) = test_admin_state().await;
        let (status, body) = registration_token(&st, "GET", &json!({})).await.unwrap();
        assert_eq!(status, 200);
        let body = body.unwrap();
        assert_eq!(body["total"], 0);
        assert!(body["tokens"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_and_list_token() {
        let (st, _dir) = test_admin_state().await;
        let (status, body) = registration_token(
            &st,
            "POST",
            &json!({
                "token": "invite-test",
                "max_uses": 5,
                "comment": "test invite"
            }),
        )
        .await
        .unwrap();
        assert_eq!(status, 201);
        let body = body.unwrap();
        assert_eq!(body["token"], "invite-test");
        assert_eq!(body["max_uses"], 5);
        assert_eq!(body["used_count"], 0);
        assert_eq!(body["comment"], "test invite");
        assert_eq!(body["status"], "active");

        let (status, body) = registration_token(&st, "GET", &json!({})).await.unwrap();
        assert_eq!(status, 200);
        let tokens = body.unwrap()["tokens"].as_array().unwrap().clone();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0]["token"], "invite-test");
        assert_eq!(tokens[0]["pending_reservations"], 0);
    }

    #[tokio::test]
    async fn create_generates_token_when_blank() {
        let (st, _dir) = test_admin_state().await;
        let (status, body) =
            registration_token(&st, "POST", &json!({ "max_uses": 2, "comment": "auto" }))
                .await
                .unwrap();
        assert_eq!(status, 201);
        let body = body.unwrap();
        let token = body["token"].as_str().unwrap();
        assert_eq!(token.len(), 24);
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[tokio::test]
    async fn create_clamps_non_positive_max_uses_to_one() {
        let (st, _dir) = test_admin_state().await;
        let (status, body) = registration_token(
            &st,
            "POST",
            &json!({ "token": "single-use", "max_uses": 0 }),
        )
        .await
        .unwrap();
        assert_eq!(status, 201);
        assert_eq!(body.unwrap()["max_uses"], 1);
    }

    #[tokio::test]
    async fn update_existing_token_preserves_used_count() {
        let (st, _dir) = test_admin_state().await;
        registration_token(
            &st,
            "POST",
            &json!({ "token": "upd", "max_uses": 2, "comment": "v1" }),
        )
        .await
        .unwrap();

        db_execute!(
            &st.pool,
            "UPDATE registration_tokens SET used_count = 1 WHERE token = ?",
            "upd"
        )
        .unwrap();

        let (status, body) = registration_token(
            &st,
            "POST",
            &json!({ "token": "upd", "max_uses": 10, "comment": "v2" }),
        )
        .await
        .unwrap();
        assert_eq!(status, 200);
        let body = body.unwrap();
        assert_eq!(body["max_uses"], 10);
        assert_eq!(body["used_count"], 1);
        assert_eq!(body["comment"], "v2");
    }

    #[tokio::test]
    async fn list_shows_pending_reservations_and_exhausted_status() {
        let (st, _dir) = test_admin_state().await;
        registration_token(&st, "POST", &json!({ "token": "pending", "max_uses": 2 }))
            .await
            .unwrap();

        ensure_new_account_quota(&st.pool, "alice@example.org")
            .await
            .unwrap();
        attach_registration_token(&st.pool, "alice@example.org", "pending")
            .await
            .unwrap();

        db_execute!(
            &st.pool,
            "UPDATE registration_tokens SET used_count = 1 WHERE token = ?",
            "pending"
        )
        .unwrap();

        let (_, body) = registration_token(&st, "GET", &json!({})).await.unwrap();
        let entry = &body.unwrap()["tokens"][0];
        assert_eq!(entry["pending_reservations"], 1);
        assert_eq!(entry["status"], "exhausted");
    }

    #[tokio::test]
    async fn delete_token_success_and_not_found() {
        let (st, _dir) = test_admin_state().await;
        registration_token(&st, "POST", &json!({ "token": "gone", "max_uses": 1 }))
            .await
            .unwrap();

        let (status, body) = registration_token(&st, "DELETE", &json!({ "token": "gone" }))
            .await
            .unwrap();
        assert_eq!(status, 200);
        assert_eq!(body.unwrap()["deleted"], "gone");

        let err = registration_token(&st, "DELETE", &json!({ "token": "gone" }))
            .await
            .unwrap_err();
        assert_eq!(err.0, 404);
    }

    #[tokio::test]
    async fn delete_rejects_empty_token() {
        let (st, _dir) = test_admin_state().await;
        let err = registration_token(&st, "DELETE", &json!({ "token": "" }))
            .await
            .unwrap_err();
        assert_eq!(err.0, 400);
        assert!(err.1.contains("token is required"));
    }

    #[tokio::test]
    async fn unsupported_method_returns_405() {
        let (st, _dir) = test_admin_state().await;
        let err = registration_token(&st, "PATCH", &json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.0, 405);
        assert!(err.1.contains("PATCH"));
    }
}
