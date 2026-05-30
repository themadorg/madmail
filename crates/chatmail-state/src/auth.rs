//! In-memory credentials cache (Madmail Go `pass_table.credCache` parity).
//!
//! Hot paths (routing, SMTP/IMAP/Web auth) must not hit the DB per recipient or
//! per login when the account is already known. Hydrate at boot and on soft reload.

use std::sync::RwLock;
use std::time::{Duration, Instant};

use chatmail_db::{
    blocklist, get_bool_setting, is_federation_rcpt_blocked, passwords, settings_keys, DbPool,
};
use chatmail_types::Result;
use dashmap::DashMap;

/// How long a successful password verification is trusted before bcrypt re-runs.
const VERIFY_CACHE_TTL: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Copy)]
struct VerifiedEntry {
    pw_sha256: [u8; 32],
    at: Instant,
}

/// Username → stored password hash (`bcrypt:…` or legacy `algo:hash`).
#[derive(Debug)]
pub struct AuthCache {
    entries: DashMap<String, String>,
    blocked: DashMap<String, ()>,
    /// Users whose `record_first_login` quota work is done (`first_login_at != 1`).
    login_settled: DashMap<String, ()>,
    /// Auth cache (Dovecot parity): username → sha256 of a password that already passed bcrypt.
    /// Delta Chat reconnects constantly; without this every IMAP/SMTP LOGIN re-runs bcrypt
    /// (cost 12 ≈ hundreds of ms of CPU), and 60 accounts reconnecting serialize into a multi
    /// second login storm on a 1-vCPU box that starves IDLE/FETCH servicing.
    verified: DashMap<String, VerifiedEntry>,
    jit_enabled: RwLock<bool>,
}

impl AuthCache {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            blocked: DashMap::new(),
            login_settled: DashMap::new(),
            verified: DashMap::new(),
            jit_enabled: RwLock::new(true),
        }
    }

    /// True if `pw_sha256` matches a recent successful verification for `username`, letting the
    /// caller skip the expensive bcrypt/argon2 check.
    pub fn check_verified(&self, username: &str, pw_sha256: &[u8; 32]) -> bool {
        if let Some(entry) = self.verified.get(username) {
            return entry.at.elapsed() < VERIFY_CACHE_TTL && &entry.pw_sha256 == pw_sha256;
        }
        false
    }

    /// Record a successful verification so subsequent reconnects with the same password are cheap.
    pub fn record_verified(&self, username: impl Into<String>, pw_sha256: [u8; 32]) {
        self.verified.insert(
            username.into(),
            VerifiedEntry {
                pw_sha256,
                at: Instant::now(),
            },
        );
    }

    /// O(1) existence check; no allocation.
    pub fn user_exists(&self, username: &str) -> bool {
        self.entries.contains_key(username)
    }

    pub fn get_hash(&self, username: &str) -> Option<String> {
        self.entries.get(username).map(|v| v.clone())
    }

    /// Write-through after DB insert/update (JIT, admin API, import).
    pub fn insert(&self, username: impl Into<String>, hash: impl Into<String>) {
        let username = username.into();
        // Password may have changed → drop any cached verification for it.
        self.verified.remove(&username);
        self.entries.insert(username, hash.into());
    }

    pub fn remove(&self, username: &str) {
        self.entries.remove(username);
        self.login_settled.remove(username);
        self.verified.remove(username);
    }

    /// True when repeat logins may skip `record_first_login` DB I/O.
    pub fn is_login_settled(&self, username: &str) -> bool {
        self.login_settled.contains_key(username)
    }

    pub fn mark_login_settled(&self, username: impl Into<String>) {
        self.login_settled.insert(username.into(), ());
    }

    pub fn is_blocked(&self, username: &str) -> bool {
        self.blocked.contains_key(username)
    }

    pub fn block(&self, username: impl Into<String>) {
        self.blocked.insert(username.into(), ());
    }

    pub fn unblock(&self, username: &str) {
        self.blocked.remove(username);
    }

    pub fn jit_registration_enabled(&self) -> bool {
        *self.jit_enabled.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether inbound mail may be delivered locally (reserved rcpt + account exists).
    pub fn local_recipient_allowed(&self, rcpt: &str) -> bool {
        if is_federation_rcpt_blocked(rcpt) {
            return false;
        }
        self.user_exists(rcpt)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Full reload from DB (boot, SIGUSR2 / admin soft reload).
    pub async fn hydrate(&self, pool: &DbPool) -> Result<()> {
        let rows = passwords::list_all_credentials(pool).await?;
        self.entries.clear();
        self.verified.clear();
        for (user, hash) in rows {
            self.entries.insert(user, hash);
        }

        let blocked = blocklist::list_blocked_users(pool).await?;
        self.blocked.clear();
        for (user, _, _) in blocked {
            self.blocked.insert(user, ());
        }

        let jit = get_bool_setting(pool, settings_keys::JIT_REGISTRATION_ENABLED, true).await?
            || get_bool_setting(pool, settings_keys::REGISTRATION_OPEN, true).await?;
        *self.jit_enabled.write().unwrap_or_else(|e| e.into_inner()) = jit;

        self.login_settled.clear();
        for user in chatmail_db::list_login_settled_usernames(pool).await? {
            self.login_settled.insert(user, ());
        }

        Ok(())
    }
}

impl Default for AuthCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::{init_memory_db, passwords};

    #[tokio::test]
    async fn hydrate_loads_all_users() {
        let pool = init_memory_db().await.unwrap();
        passwords::create_user(&pool, "a@test", "bcrypt:1")
            .await
            .unwrap();
        passwords::create_user(&pool, "b@test", "bcrypt:2")
            .await
            .unwrap();

        let cache = AuthCache::new();
        cache.hydrate(&pool).await.unwrap();
        assert_eq!(cache.len(), 2);
        assert!(cache.user_exists("a@test"));
        assert_eq!(cache.get_hash("b@test").as_deref(), Some("bcrypt:2"));
    }

    #[tokio::test]
    async fn hydrate_loads_blocklist_and_jit_flag() {
        let pool = init_memory_db().await.unwrap();
        blocklist::block_user(&pool, "bad@test", "test").await.unwrap();
        chatmail_db::set_setting(
            &pool,
            settings_keys::JIT_REGISTRATION_ENABLED,
            "false",
        )
        .await
        .unwrap();
        chatmail_db::set_setting(&pool, settings_keys::REGISTRATION_OPEN, "false")
            .await
            .unwrap();

        let cache = AuthCache::new();
        cache.hydrate(&pool).await.unwrap();
        assert!(cache.is_blocked("bad@test"));
        assert!(!cache.jit_registration_enabled());
    }

    #[test]
    fn local_recipient_blocks_reserved_and_unknown() {
        let cache = AuthCache::new();
        cache.insert("u@test", "h");
        assert!(!cache.local_recipient_allowed("admin@test"));
        assert!(!cache.local_recipient_allowed("ghost@test"));
        assert!(cache.local_recipient_allowed("u@test"));
    }

    #[test]
    fn insert_remove_roundtrip() {
        let cache = AuthCache::new();
        cache.insert("u@test", "bcrypt:x");
        assert!(cache.user_exists("u@test"));
        cache.remove("u@test");
        assert!(!cache.user_exists("u@test"));
    }

    #[test]
    fn block_unblock_roundtrip() {
        let cache = AuthCache::new();
        cache.block("u@test");
        assert!(cache.is_blocked("u@test"));
        cache.unblock("u@test");
        assert!(!cache.is_blocked("u@test"));
    }
}
