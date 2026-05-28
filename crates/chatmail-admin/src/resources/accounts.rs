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

//! `/admin/accounts` — Madmail `resources.AccountsHandler`.

use chatmail_auth::{hash_password, is_importable_hash, normalize_username};
use chatmail_db::{
    account_info, blocklist, passwords, registration_tokens, AccountQuotaInfo,
    ADMIN_DELETE_REASON, BULK_DELETE_REASON,
};
use getrandom::getrandom;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{status_storage::db_err, AdminResult};
use crate::AdminState;

/// Madmail admin `POST` uses 12-char localparts and 24-char passwords.
const ADMIN_USERNAME_LEN: usize = 12;
const ADMIN_PASSWORD_LEN: usize = 24;

#[derive(Deserialize)]
struct UsernameBody {
    username: String,
}

#[derive(Deserialize)]
struct BulkBody {
    action: String,
    #[serde(default)]
    users: Vec<ImportUser>,
}

#[derive(Deserialize)]
struct ImportUser {
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    hash: String,
}

fn normalize_account_username(raw: &str) -> Result<String, (u16, String)> {
    normalize_username(raw.trim()).map_err(|e| (400, e.to_string()))
}

fn is_internal_settings_key(username: &str) -> bool {
    username.starts_with("__") && username.ends_with("__")
}

fn random_alnum(len: usize) -> Result<String, (u16, String)> {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut b = vec![0u8; len];
    getrandom(&mut b).map_err(|e| (500, format!("failed to generate random string: {e}")))?;
    Ok(b
        .iter()
        .map(|x| CHARSET[(*x as usize) % CHARSET.len()] as char)
        .collect())
}

fn random_password(len: usize) -> Result<String, (u16, String)> {
    const CHARSET: &[u8] =
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()_+-=[]{}|;:,.<>?";
    let mut b = vec![0u8; len];
    getrandom(&mut b).map_err(|e| (500, format!("failed to generate password: {e}")))?;
    Ok(b
        .iter()
        .map(|x| CHARSET[(*x as usize) % CHARSET.len()] as char)
        .collect())
}

async fn delete_account_full(
    st: &AdminState,
    username: &str,
    reason: &str,
) -> Result<(), (u16, String)> {
    let maildir = st.app.mailbox_store.maildir_for_user(username);
    if maildir.root.exists() {
        tokio::fs::remove_dir_all(&maildir.root)
            .await
            .map_err(db_err)?;
    }
    if let Err(e) = passwords::delete_user(&st.pool, username).await {
        tracing::warn!(%username, error = %e, "admin delete: credentials removal failed");
    }
    chatmail_db::account_info::delete_quota_row(&st.pool, username)
        .await
        .map_err(db_err)?;
    blocklist::block_user(&st.pool, username, reason)
        .await
        .map_err(db_err)?;
    st.app.quota.invalidate(username);
    Ok(())
}

async fn provision_account(
    st: &AdminState,
    username: &str,
    stored_hash: &str,
) -> Result<(), (u16, String)> {
    passwords::create_user(&st.pool, username, stored_hash)
        .await
        .map_err(db_err)?;
    st.app
        .mailbox_store
        .init_user_dir(username)
        .await
        .map_err(db_err)?;
    registration_tokens::ensure_new_account_quota(&st.pool, username)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// Madmail `GET /admin/accounts` — quota usage + `quotas` login timestamps.
async fn list_accounts(st: &AdminState) -> AdminResult {
    let users = passwords::list_users(&st.pool).await.map_err(db_err)?;
    let info = account_info::list_account_quota_info(&st.pool)
        .await
        .map_err(db_err)?;
    let mut accounts = Vec::new();
    for u in users {
        if is_internal_settings_key(&u) {
            continue;
        }
        let (used, max, is_default) = st.app.quota.get_quota(&u);
        let AccountQuotaInfo {
            created_at,
            first_login_at,
            last_login_at,
        } = info.get(&u).copied().unwrap_or_default();
        accounts.push(json!({
            "username": u,
            "used_bytes": used,
            "max_bytes": max,
            "is_default_quota": is_default,
            "created_at": created_at,
            "first_login_at": first_login_at,
            "last_login_at": last_login_at,
        }));
    }
    let total = accounts.len();
    Ok((200, Some(json!({ "accounts": accounts, "total": total }))))
}

pub async fn accounts(st: &AdminState, method: &str, body: &Value) -> AdminResult {
    match method {
        "GET" => list_accounts(st).await,
        "POST" => {
            if st.mail_domain.is_empty() {
                return Err((
                    503,
                    "account creation not configured (no mail domain)".into(),
                ));
            }

            const MAX_ATTEMPTS: u32 = 5;
            for _ in 0..MAX_ATTEMPTS {
                let localpart = random_alnum(ADMIN_USERNAME_LEN)?;
                let password = random_password(ADMIN_PASSWORD_LEN)?;
                let email = format!("{localpart}@{}", st.mail_domain);

                if blocklist::is_blocked(&st.pool, &email).await.map_err(db_err)? {
                    continue;
                }

                if passwords::user_exists(&st.pool, &email).await.map_err(db_err)? {
                    continue;
                }

                let hash = hash_password(&password).map_err(db_err)?;
                match provision_account(st, &email, &hash).await {
                    Ok(()) => {
                        return Ok((
                            201,
                            Some(json!({ "email": email, "password": password })),
                        ));
                    }
                    Err((500, _)) => continue,
                    Err(e) => return Err(e),
                }
            }
            Err((
                500,
                "failed to create account after max retries".into(),
            ))
        }
        "DELETE" => {
            let req: UsernameBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            if req.username.is_empty() {
                return Err((400, "username is required".into()));
            }
            let username = normalize_account_username(&req.username)?;
            delete_account_full(st, &username, ADMIN_DELETE_REASON).await?;
            Ok((
                200,
                Some(json!({
                    "deleted": username,
                    "blocked": username,
                    "reason": ADMIN_DELETE_REASON,
                })),
            ))
        }
        "PATCH" => {
            let req: BulkBody = serde_json::from_value(body.clone())
                .map_err(|e| (400, e.to_string()))?;
            match req.action.as_str() {
                "export" => export_accounts(st).await,
                "import" => import_accounts(st, req.users).await,
                "delete_all" => delete_all_accounts(st).await,
                _ => Err((
                    400,
                    format!(
                        "unknown bulk action: {} (expected: export, import, delete_all)",
                        req.action
                    ),
                )),
            }
        }
        _ => Err((405, format!("method {method} not allowed for /admin/accounts"))),
    }
}

async fn export_accounts(st: &AdminState) -> AdminResult {
    let users = passwords::list_users(&st.pool).await.map_err(db_err)?;
    let mut entries = Vec::new();
    for u in users {
        if is_internal_settings_key(&u) {
            continue;
        }
        let hash = passwords::get_user_hash(&st.pool, &u).await.map_err(db_err)?;
        entries.push(json!({
            "username": u,
            "hash": hash.unwrap_or_default(),
        }));
    }
    let total = entries.len();
    Ok((200, Some(json!({ "users": entries, "total": total }))))
}

async fn import_accounts(st: &AdminState, users: Vec<ImportUser>) -> AdminResult {
    if users.is_empty() {
        return Err((400, "users array is empty".into()));
    }

    let mut imported = 0i32;
    let mut skipped = 0i32;
    let mut errors = Vec::new();

    for u in users {
        if u.username.is_empty() {
            skipped += 1;
            errors.push("skipped entry with empty username".into());
            continue;
        }
        if is_internal_settings_key(&u.username) {
            skipped += 1;
            continue;
        }

        let username = match normalize_account_username(&u.username) {
            Ok(n) => n,
            Err((_, msg)) => {
                errors.push(format!("{}: {msg}", u.username));
                continue;
            }
        };

        if passwords::user_exists(&st.pool, &username).await.map_err(db_err)? {
            skipped += 1;
            continue;
        }

        if blocklist::is_blocked(&st.pool, &username).await.map_err(db_err)? {
            skipped += 1;
            errors.push(format!("{username}: username is blocklisted"));
            continue;
        }

        let stored_hash = if !u.hash.is_empty() {
            if !is_importable_hash(&u.hash) {
                errors.push(format!("{username}: unsupported password hash format"));
                skipped += 1;
                continue;
            }
            u.hash
        } else {
            let password = if u.password.is_empty() {
                random_password(ADMIN_PASSWORD_LEN)?
            } else {
                u.password
            };
            hash_password(&password).map_err(db_err)?
        };

        if let Err((_, msg)) = provision_account(st, &username, &stored_hash).await {
            let _ = passwords::delete_user(&st.pool, &username).await;
            let _ = chatmail_db::account_info::delete_quota_row(&st.pool, &username).await;
            errors.push(format!("{username}: {msg}"));
            continue;
        }
        imported += 1;
    }

    let mut resp = json!({ "imported": imported, "skipped": skipped });
    if !errors.is_empty() {
        resp["errors"] = json!(errors);
    }
    Ok((200, Some(resp)))
}

async fn delete_all_accounts(st: &AdminState) -> AdminResult {
    let users = passwords::list_users(&st.pool).await.map_err(db_err)?;
    let mut deleted = 0i32;
    let mut errors = Vec::new();

    for u in users {
        if is_internal_settings_key(&u) {
            continue;
        }
        match delete_account_full(st, &u, BULK_DELETE_REASON).await {
            Ok(()) => deleted += 1,
            Err((_, msg)) => errors.push(format!("{u}: {msg}")),
        }
    }

    let mut resp = json!({ "deleted": deleted });
    if !errors.is_empty() {
        resp["errors"] = json!(errors);
    }
    Ok((200, Some(resp)))
}
