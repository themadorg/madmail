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

use std::sync::Arc;

use chatmail_config::CredentialPolicy;
use chatmail_db::{
    blocklist, get_bool_setting, passwords, registration_tokens, settings_keys,
    FirstLoginOutcome, DbPool,
};
use chatmail_state::AppState;
use chatmail_storage::MailboxStore;
use chatmail_types::{ChatmailError, Result};

use chatmail_types::validate_login_domain;

use crate::hash::{hash_password, verify_password};
use crate::normalize::normalize_username;
use crate::validate::validate_localpart_and_password;

pub struct AuthContext {
    pub pool: DbPool,
    pub state: Arc<AppState>,
    pub primary_domain: String,
    /// `auth.pass_table` `jit_domain` — restrict JIT/login to this domain (often `[ip]`).
    pub jit_domain: Option<String>,
    /// `chatmail` credential length limits from `maddy.conf`.
    pub credential_policy: CredentialPolicy,
}

impl AuthContext {
    pub fn mailbox_store(&self) -> &MailboxStore {
        &self.state.mailbox_store
    }
}

/// Authenticate user; JIT-create account when enabled (Madmail pass_table + imapsql).
pub async fn authenticate(
    ctx: &AuthContext,
    username: &str,
    password: &str,
) -> Result<()> {
    let user = normalize_username(username)?;

    if let Some(ref jit) = ctx.jit_domain {
        if !jit.is_empty() {
            validate_login_domain(&user, jit)
                .map_err(|e| ChatmailError::config(e))?;
        }
    }

    if blocklist::is_blocked(&ctx.pool, &user).await? {
        return Err(ChatmailError::UserBlocked(user));
    }

    if let Some(hash) = passwords::get_user_hash(&ctx.pool, &user).await? {
        if verify_password(password, &hash)? {
            return finish_successful_login(ctx, &user).await;
        }
        return Err(ChatmailError::AuthFailed);
    }

    let jit = jit_enabled(&ctx.pool).await?;
    if !jit {
        return Err(ChatmailError::AuthFailed);
    }

    validate_localpart_and_password(&ctx.credential_policy, &user, password)?;

    let hash = hash_password(password)?;
    passwords::create_user(&ctx.pool, &user, &hash).await?;
    ctx.state.mailbox_store.init_user_dir(&user).await?;
    registration_tokens::ensure_new_account_quota(&ctx.pool, &user).await?;

    finish_successful_login(ctx, &user).await
}

async fn finish_successful_login(ctx: &AuthContext, user: &str) -> Result<()> {
    match registration_tokens::record_first_login(&ctx.pool, user).await? {
        FirstLoginOutcome::Ok => Ok(()),
        FirstLoginOutcome::AccountRemoved => {
            let maildir = ctx.mailbox_store().maildir_for_user(user);
            if maildir.root.exists() {
                let _ = tokio::fs::remove_dir_all(&maildir.root).await;
            }
            let _ = passwords::delete_user(&ctx.pool, user).await;
            Err(ChatmailError::AuthFailed)
        }
    }
}

async fn jit_enabled(pool: &DbPool) -> Result<bool> {
    if get_bool_setting(pool, settings_keys::JIT_REGISTRATION_ENABLED, true).await? {
        return Ok(true);
    }
    if get_bool_setting(pool, settings_keys::REGISTRATION_OPEN, true).await? {
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::{init_memory_db, set_setting};
    use chatmail_state::AppState;

    async fn ctx_with_jit(jit: bool) -> (AuthContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_memory_db().await.unwrap();
        set_setting(
            &pool,
            settings_keys::JIT_REGISTRATION_ENABLED,
            if jit { "true" } else { "false" },
        )
        .await
        .unwrap();
        set_setting(
            &pool,
            settings_keys::REGISTRATION_OPEN,
            if jit { "true" } else { "false" },
        )
        .await
        .unwrap();
        let state = Arc::new(AppState::new(dir.path()));
        let ctx = AuthContext {
            pool,
            state,
            primary_domain: "example.org".into(),
            jit_domain: Some("example.org".into()),
            credential_policy: CredentialPolicy::default(),
        };
        (ctx, dir)
    }

    /// P3-UT03
    #[tokio::test]
    async fn p3_ut03_test_jit_creates_user() {
        let (ctx, _dir) = ctx_with_jit(true).await;
        authenticate(&ctx, "newuser1@example.org", "longpassword").await.unwrap();
        assert!(passwords::get_user_hash(&ctx.pool, "newuser1@example.org")
            .await
            .unwrap()
            .is_some());
    }

    /// P3-UT04: blocked users cannot authenticate even with correct password.
    #[tokio::test]
    async fn p3_ut04_test_blocked_user_rejected() {
        let (ctx, _dir) = ctx_with_jit(true).await;
        passwords::create_user(&ctx.pool, "blocked@example.org", "bcrypt:x")
            .await
            .unwrap();
        chatmail_db::blocklist::block_user(&ctx.pool, "blocked@example.org", "test")
            .await
            .unwrap();
        assert!(matches!(
            authenticate(&ctx, "blocked@example.org", "pw").await,
            Err(ChatmailError::UserBlocked(_))
        ));
    }

    /// P3-UT04 (plan): JIT disabled rejects unknown users.
    #[tokio::test]
    async fn p3_ut04_test_jit_disabled_rejects() {
        let (ctx, _dir) = ctx_with_jit(false).await;
        assert!(matches!(
            authenticate(&ctx, "missing@example.org", "pw").await,
            Err(ChatmailError::AuthFailed)
        ));
    }

    /// P3-UT05: JIT create enforces min username/password from credential policy.
    #[tokio::test]
    async fn p3_ut05_jit_rejects_short_localpart() {
        let (ctx, _dir) = ctx_with_jit(true).await;
        let err = authenticate(&ctx, "ab@example.org", "longpassword1").await.unwrap_err();
        assert!(matches!(err, ChatmailError::Config(msg) if msg.contains("between 8 and 20")));
        assert!(
            passwords::get_user_hash(&ctx.pool, "ab@example.org")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn p3_ut05_jit_rejects_short_password() {
        let (ctx, _dir) = ctx_with_jit(true).await;
        let err = authenticate(&ctx, "validuser@example.org", "short").await.unwrap_err();
        assert!(matches!(err, ChatmailError::Config(msg) if msg.contains("at least 8")));
        assert!(
            passwords::get_user_hash(&ctx.pool, "validuser@example.org")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn p3_ut05_existing_user_login_skips_length_check() {
        let (ctx, _dir) = ctx_with_jit(true).await;
        let hash = crate::hash_password("x").unwrap();
        passwords::create_user(&ctx.pool, "legacy@example.org", &hash)
            .await
            .unwrap();
        authenticate(&ctx, "legacy@example.org", "x").await.unwrap();
    }
}
