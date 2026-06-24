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

pub mod account_info;
pub mod blocklist;
pub mod endpoint_cache;
pub mod federation_policy;
pub mod inbound;
pub mod mail_ports;
pub mod maintenance;
pub mod message_retention;
pub mod message_stats;
pub mod models;
pub mod modseq;
pub mod passwords;
pub mod pool;
pub mod quota_defaults;
pub mod registration_tokens;
pub mod schema;
pub mod settings;
pub mod settings_keys;
pub mod sharing;

use std::path::Path;

use chatmail_config::DatabaseConfig;
use chatmail_types::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

pub use account_info::{delete_quota_row, list_account_quota_info, AccountQuotaInfo};
pub use blocklist::{
    block_user, is_blocked, list_blocked_users, unblock_user, ADMIN_DELETE_REASON,
    BULK_DELETE_REASON, CLI_BAN_REASON, CLI_DELETE_REASON, MANUAL_BLOCK_REASON,
};
pub use endpoint_cache::{
    get_endpoint_override, list_endpoint_overrides, remove_endpoint_override,
    set_endpoint_override, EndpointOverrideRow,
};
pub use federation_policy::{
    federation_policy_label, normalize_federation_domain, set_federation_policy_label,
};
pub use inbound::{
    inbound_local_recipient_allowed, is_federation_rcpt_blocked, is_federation_sender_blocked,
};
pub use mail_ports::{db_ports_from_settings, load_mail_port_overrides};
pub use maintenance::{list_dormant_accounts, remove_account_without_blocklist};
pub use message_retention::{
    duration_from_value, effective_message_retention, format_retention_days,
    message_retention_enabled, message_retention_status, retention_days_from_value,
    MessageRetentionStatus, DEFAULT_RETENTION_DAYS,
};
pub use message_stats::{
    hydrate as hydrate_message_stats, increment_outbound, increment_received, increment_sent,
    record_inbound_delivery, record_smtp_accepted, snapshot as message_stats_snapshot,
    start_flush_task as start_message_stats_flush,
};
pub use modseq::{load_all_modseq, upsert_modseq};
pub use pool::{connect_database, pg_sql, DbBackend, DbPool};
pub use quota_defaults::resolve_default_quota_bytes;
pub use registration_tokens::{
    attach_registration_token, ensure_new_account_quota, list_login_settled_usernames,
    record_first_login, reserve_registration_token, validate_registration_token, FirstLoginOutcome,
};
pub use settings::{
    delete_setting, get_bool_setting, get_enabled_setting, get_setting, get_settings_many,
    list_double_underscore_settings, seed_install_defaults, set_setting,
};
pub use sharing::{
    create_sharing_contact, get_sharing_contact, init_sharing_db, list_sharing_contacts,
    normalize_sharing_url, remove_sharing_contact, sharing_slug_exists, update_sharing_contact,
    validate_slug, SharingContact,
};

/// Open (or create) the application database and run embedded migrations.
pub async fn init_db_from_config(config: &DatabaseConfig) -> Result<DbPool> {
    let pool = connect_database(config).await?;
    pool::run_migrations(&pool).await?;
    settings::seed_install_defaults(&pool).await?;
    message_stats::hydrate(&pool).await?;
    message_stats::start_flush_task(pool.clone());
    Ok(pool)
}

/// Open (or create) a SQLite database file (tests / legacy callers).
pub async fn init_db(db_path: &Path) -> Result<DbPool> {
    let config = DatabaseConfig {
        driver: chatmail_config::DbDriver::Sqlite3,
        dsn: db_path.display().to_string(),
    };
    init_db_from_config(&config).await
}

/// In-memory SQLite database for unit tests.
pub async fn init_memory_db() -> Result<DbPool> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?.create_if_missing(true);

    let sqlite = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;

    let pool = DbPool::Sqlite(sqlite);
    pool::run_migrations(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED_TABLES: &[&str] = &[
        "settings",
        "quotas",
        "blocked_users",
        "registration_tokens",
        "dns_overrides",
        "federation_server_stats",
        "federation_rules",
        "federation_silent_dismiss",
        "message_stats",
        "exchangers",
        "passwords",
        "push_tokens",
    ];

    /// P1-UT03: migrations are idempotent on the same pool.
    #[tokio::test]
    async fn p1_ut03_db_migration_idempotency() {
        let pool = init_memory_db().await.expect("first init");
        pool::run_migrations(&pool).await.expect("second migrate");
    }

    /// P1-UT03: file-backed DB applies migrations and creates expected tables.
    #[tokio::test]
    async fn p1_ut03_init_db_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("chatmail.db");
        let pool = init_db(&db_path).await.unwrap();
        assert!(db_path.is_file());
        assert_schema_tables(&pool).await;
    }

    /// P1-UT03: in-memory DB has all Phase 1 tables.
    #[tokio::test]
    async fn p1_ut03_schema_tables_exist() {
        let pool = init_memory_db().await.unwrap();
        assert_schema_tables(&pool).await;
    }

    #[tokio::test]
    async fn federation_stats_columns_sqlite_uses_https_names() {
        let pool = init_memory_db().await.unwrap();
        let cols = schema::federation_stats_columns(&pool).await.unwrap();
        assert_eq!(cols.failed_https, "failed_https");
        assert_eq!(cols.success_https, "success_https");
    }

    async fn assert_schema_tables(pool: &DbPool) {
        for table in EXPECTED_TABLES {
            let exists = schema::table_exists(pool, table).await.unwrap();
            assert!(exists, "missing table {table}");
        }
    }
}
