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

//! Individual maintenance jobs.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chatmail_config::parse_duration;
use chatmail_db::{
    get_enabled_setting, list_dormant_accounts, remove_account_without_blocklist, settings_keys,
    DbPool,
};
use chatmail_storage::{
    prune_unread_older, purge_mail_blobs_older, purge_read_messages, MailboxStore,
};
use chatmail_types::{ChatmailError, Result};

use crate::config::MaintenanceConfig;

/// Named maintenance job (CLI + scheduler).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskId {
    PruneOldMessages,
    PruneUnusedAccounts,
    PurgeSeenMessages,
    PruneUnreadOlder,
}

impl TaskId {
    pub const ALL: &'static [TaskId] = &[
        TaskId::PruneOldMessages,
        TaskId::PruneUnusedAccounts,
        TaskId::PurgeSeenMessages,
        TaskId::PruneUnreadOlder,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "prune-old-messages" | "prune-messages" | "retention" => {
                Some(TaskId::PruneOldMessages)
            }
            "prune-unused-accounts" | "prune-unused" | "unused-accounts" => {
                Some(TaskId::PruneUnusedAccounts)
            }
            "purge-seen" | "purge-read" | "auto-purge-seen" => Some(TaskId::PurgeSeenMessages),
            "prune-unread-older" | "purge-unread-older" => Some(TaskId::PruneUnreadOlder),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TaskId::PruneOldMessages => "prune-old-messages",
            TaskId::PruneUnusedAccounts => "prune-unused-accounts",
            TaskId::PurgeSeenMessages => "purge-seen",
            TaskId::PruneUnreadOlder => "prune-unread-older",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            TaskId::PruneOldMessages => {
                "Delete maildir message files older than storage.imapsql retention"
            }
            TaskId::PruneUnusedAccounts => {
                "Delete accounts that never logged in, older than unused_account_retention"
            }
            TaskId::PurgeSeenMessages => "Delete maildir cur/ (seen) messages",
            TaskId::PruneUnreadOlder => {
                "Delete maildir new/ messages older than --retention"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskOutcome {
    pub task: TaskId,
    pub deleted: usize,
    pub skipped: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Default)]
pub struct TaskRunReport {
    pub outcomes: Vec<TaskOutcome>,
}

impl TaskRunReport {
    pub fn push(&mut self, outcome: TaskOutcome) {
        self.outcomes.push(outcome);
    }
}

pub struct TaskContext<'a> {
    pub pool: &'a DbPool,
    pub mailbox: &'a MailboxStore,
    pub maintenance: &'a MaintenanceConfig,
}

/// Run one job. `retention_override` applies to retention-based tasks when set.
pub async fn run_task(
    ctx: &TaskContext<'_>,
    task: TaskId,
    retention_override: Option<Duration>,
) -> Result<TaskOutcome> {
    match task {
        TaskId::PruneOldMessages => prune_old_messages(ctx, retention_override).await,
        TaskId::PruneUnusedAccounts => prune_unused_accounts(ctx, retention_override).await,
        TaskId::PurgeSeenMessages => purge_seen(ctx).await,
        TaskId::PruneUnreadOlder => {
            let retention = retention_override.ok_or_else(|| {
                ChatmailError::config(
                    "prune-unread-older requires --retention (e.g. 24h) or storage.imapsql retention in config",
                )
            })?;
            prune_unread_older_job(ctx, retention).await
        }
    }
}

/// Run all jobs that are enabled by static config (ignores DB auto-purge toggle).
pub async fn run_all_configured(ctx: &TaskContext<'_>) -> Result<TaskRunReport> {
    let mut report = TaskRunReport::default();
    if ctx.maintenance.message_retention.is_some() {
        report.push(run_task(ctx, TaskId::PruneOldMessages, None).await?);
    }
    if ctx.maintenance.unused_account_retention.is_some() {
        report.push(run_task(ctx, TaskId::PruneUnusedAccounts, None).await?);
    }
    Ok(report)
}

pub async fn run_auto_purge_seen_if_enabled(pool: &DbPool, mailbox: &MailboxStore) -> Result<Option<usize>> {
    if !get_enabled_setting(pool, settings_keys::AUTO_PURGE_SEEN, false).await? {
        return Ok(None);
    }
    let deleted = purge_read_messages(mailbox).await?;
    Ok(Some(deleted))
}

async fn prune_old_messages(
    ctx: &TaskContext<'_>,
    retention_override: Option<Duration>,
) -> Result<TaskOutcome> {
    let retention = retention_override.or(ctx.maintenance.message_retention);
    let Some(retention) = retention else {
        return Ok(TaskOutcome {
            task: TaskId::PruneOldMessages,
            deleted: 0,
            skipped: true,
            detail: Some("storage.imapsql retention not set (or 0)".into()),
        });
    };
    let deleted = purge_mail_blobs_older(ctx.mailbox, retention).await?;
    Ok(TaskOutcome {
        task: TaskId::PruneOldMessages,
        deleted,
        skipped: false,
        detail: Some(format!("retention {:?}", retention)),
    })
}

async fn prune_unused_accounts(
    ctx: &TaskContext<'_>,
    retention_override: Option<Duration>,
) -> Result<TaskOutcome> {
    let retention = retention_override.or(ctx.maintenance.unused_account_retention);
    let Some(retention) = retention else {
        return Ok(TaskOutcome {
            task: TaskId::PruneUnusedAccounts,
            deleted: 0,
            skipped: true,
            detail: Some("storage.imapsql unused_account_retention not set (or 0)".into()),
        });
    };
    let deleted = prune_unused_accounts_with_retention(ctx.pool, ctx.mailbox, retention).await?;
    Ok(TaskOutcome {
        task: TaskId::PruneUnusedAccounts,
        deleted,
        skipped: false,
        detail: Some(format!("retention {:?}", retention)),
    })
}

async fn purge_seen(ctx: &TaskContext<'_>) -> Result<TaskOutcome> {
    let deleted = purge_read_messages(ctx.mailbox).await?;
    Ok(TaskOutcome {
        task: TaskId::PurgeSeenMessages,
        deleted,
        skipped: false,
        detail: None,
    })
}

async fn prune_unread_older_job(ctx: &TaskContext<'_>, retention: Duration) -> Result<TaskOutcome> {
    let deleted = prune_unread_older(ctx.mailbox, retention).await?;
    Ok(TaskOutcome {
        task: TaskId::PruneUnreadOlder,
        deleted,
        skipped: false,
        detail: Some(format!("retention {:?}", retention)),
    })
}

pub async fn prune_unused_accounts_with_retention(
    pool: &DbPool,
    mailbox: &MailboxStore,
    retention: Duration,
) -> Result<usize> {
    let cutoff = unix_now().saturating_sub(retention.as_secs() as i64);
    let accounts = list_dormant_accounts(pool, cutoff).await?;
    let mut deleted = 0usize;
    for username in accounts {
        let mail_root = mailbox.maildir_for_user(&username).root;
        if remove_account_without_blocklist(pool, &username, &mail_root).await.is_ok() {
            deleted += 1;
        }
    }
    Ok(deleted)
}

pub fn parse_retention_arg(s: &str) -> Result<Duration> {
    parse_duration(s.trim()).map_err(|_| {
        ChatmailError::config(format!(
            "invalid --retention {s:?} (use Go-style durations: 24h, 7d, 720h)"
        ))
    })
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use filetime::{set_file_mtime, FileTime};
    use super::*;
    use chatmail_config::AppConfig;
    use chatmail_db::{init_memory_db, passwords, registration_tokens};
    use chatmail_storage::{list_inbox, write_blob, MailboxStore};

    fn touch_unix_epoch(path: &Path) {
        set_file_mtime(path, FileTime::from_unix_time(1, 0)).unwrap();
    }

    async fn seed_dormant_account(pool: &DbPool, username: &str, created_at: i64) {
        passwords::create_user(pool, username, "hash").await.unwrap();
        registration_tokens::ensure_new_account_quota(pool, username)
            .await
            .unwrap();
        chatmail_db::db_execute!(
            pool,
            "UPDATE quotas SET created_at = ? WHERE username = ?",
            created_at,
            username
        )
        .unwrap();
    }

    async fn seed_logged_in_account(pool: &DbPool, username: &str, created_at: i64) {
        seed_dormant_account(pool, username, created_at).await;
        let login_at = 1_700_000_000_i64;
        chatmail_db::db_execute!(
            pool,
            "UPDATE quotas SET first_login_at = ?, last_login_at = ? WHERE username = ?",
            login_at,
            login_at,
            username
        )
        .unwrap();
    }

    #[test]
    fn task_id_aliases() {
        assert_eq!(TaskId::parse("prune-unused"), Some(TaskId::PruneUnusedAccounts));
        assert_eq!(TaskId::parse("retention"), Some(TaskId::PruneOldMessages));
    }

    #[tokio::test]
    async fn prune_old_messages_skipped_without_config() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mailbox = MailboxStore::new(dir.path());
        let maintenance = MaintenanceConfig::from_app_config(&AppConfig::default()).unwrap();
        let ctx = TaskContext {
            pool: &pool,
            mailbox: &mailbox,
            maintenance: &maintenance,
        };
        let out = run_task(&ctx, TaskId::PruneOldMessages, None).await.unwrap();
        assert!(out.skipped);
    }

    /// End-to-end: dormant account + maildir removed; logged-in account kept.
    #[tokio::test]
    async fn prune_unused_accounts_removes_dormant_and_maildir_keeps_active() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mailbox = MailboxStore::new(dir.path());

        seed_dormant_account(&pool, "dormant@test", 1).await;
        seed_logged_in_account(&pool, "active@test", 1).await;

        mailbox.init_user_dir("dormant@test").await.unwrap();
        write_blob(&mailbox, "dormant@test", "m1", b"bye").await.unwrap();
        mailbox.init_user_dir("active@test").await.unwrap();
        write_blob(&mailbox, "active@test", "m2", b"stay").await.unwrap();

        let deleted =
            prune_unused_accounts_with_retention(&pool, &mailbox, Duration::from_secs(3600))
                .await
                .unwrap();
        assert_eq!(deleted, 1);

        assert!(!passwords::user_exists(&pool, "dormant@test").await.unwrap());
        assert!(!mailbox.maildir_for_user("dormant@test").root.exists());
        assert!(list_inbox(&mailbox, "dormant@test").await.unwrap().is_empty());

        assert!(passwords::user_exists(&pool, "active@test").await.unwrap());
        assert_eq!(list_inbox(&mailbox, "active@test").await.unwrap().len(), 1);
        assert!(
            !chatmail_db::blocklist::is_blocked(&pool, "dormant@test")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn run_task_prune_unused_accounts_with_retention_override() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mailbox = MailboxStore::new(dir.path());
        let maintenance = MaintenanceConfig::from_app_config(&AppConfig::default()).unwrap();
        seed_dormant_account(&pool, "old@test", 1).await;

        let ctx = TaskContext {
            pool: &pool,
            mailbox: &mailbox,
            maintenance: &maintenance,
        };
        let out = run_task(
            &ctx,
            TaskId::PruneUnusedAccounts,
            Some(Duration::from_secs(3600)),
        )
        .await
        .unwrap();
        assert!(!out.skipped);
        assert_eq!(out.deleted, 1);
        assert!(!passwords::user_exists(&pool, "old@test").await.unwrap());
    }

    #[tokio::test]
    async fn run_task_prune_old_messages_deletes_stale_maildir_files() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mailbox = MailboxStore::new(dir.path());
        let maintenance = MaintenanceConfig::from_app_config(&AppConfig::default()).unwrap();
        let paths = mailbox.init_user_dir("u@test").await.unwrap();
        write_blob(&mailbox, "u@test", "stale", b"o").await.unwrap();
        write_blob(&mailbox, "u@test", "fresh", b"n").await.unwrap();
        touch_unix_epoch(&paths.new.join("stale"));

        let ctx = TaskContext {
            pool: &pool,
            mailbox: &mailbox,
            maintenance: &maintenance,
        };
        let out = run_task(
            &ctx,
            TaskId::PruneOldMessages,
            Some(Duration::from_secs(3600)),
        )
        .await
        .unwrap();
        assert!(!out.skipped);
        assert_eq!(out.deleted, 1);
        let left = list_inbox(&mailbox, "u@test").await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].msg_id, "fresh");
    }

    #[tokio::test]
    async fn run_task_purge_seen_deletes_cur_only() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mailbox = MailboxStore::new(dir.path());
        let maintenance = MaintenanceConfig::from_app_config(&AppConfig::default()).unwrap();
        let paths = mailbox.init_user_dir("u@test").await.unwrap();
        write_blob(&mailbox, "u@test", "unread", b"n").await.unwrap();
        write_blob(&mailbox, "u@test", "read", b"r").await.unwrap();
        tokio::fs::rename(paths.new.join("read"), paths.cur.join("read"))
            .await
            .unwrap();

        let ctx = TaskContext {
            pool: &pool,
            mailbox: &mailbox,
            maintenance: &maintenance,
        };
        let out = run_task(&ctx, TaskId::PurgeSeenMessages, None)
            .await
            .unwrap();
        assert!(!out.skipped);
        assert_eq!(out.deleted, 1);
        let left = list_inbox(&mailbox, "u@test").await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].msg_id, "unread");
    }

    #[tokio::test]
    async fn run_task_prune_unread_older_respects_retention() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mailbox = MailboxStore::new(dir.path());
        let maintenance = MaintenanceConfig::from_app_config(&AppConfig::default()).unwrap();
        let paths = mailbox.init_user_dir("u@test").await.unwrap();
        write_blob(&mailbox, "u@test", "stale", b"s").await.unwrap();
        write_blob(&mailbox, "u@test", "fresh", b"f").await.unwrap();
        touch_unix_epoch(&paths.new.join("stale"));

        let ctx = TaskContext {
            pool: &pool,
            mailbox: &mailbox,
            maintenance: &maintenance,
        };
        let out = run_task(
            &ctx,
            TaskId::PruneUnreadOlder,
            Some(Duration::from_secs(3600)),
        )
        .await
        .unwrap();
        assert!(!out.skipped);
        assert_eq!(out.deleted, 1);
        let left = list_inbox(&mailbox, "u@test").await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].msg_id, "fresh");
    }

    #[tokio::test]
    async fn run_all_configured_executes_retention_jobs_from_config() {
        let pool = init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mailbox = MailboxStore::new(dir.path());

        let mut cfg = AppConfig::default();
        cfg.retention = Some("24h".into());
        cfg.unused_account_retention = Some("720h".into());
        let maintenance = MaintenanceConfig::from_app_config(&cfg).unwrap();

        seed_dormant_account(&pool, "gone@test", 1).await;
        let paths = mailbox.init_user_dir("u@test").await.unwrap();
        write_blob(&mailbox, "u@test", "old", b"o").await.unwrap();
        touch_unix_epoch(&paths.new.join("old"));

        let ctx = TaskContext {
            pool: &pool,
            mailbox: &mailbox,
            maintenance: &maintenance,
        };
        let report = run_all_configured(&ctx).await.unwrap();
        assert_eq!(report.outcomes.len(), 2);
        assert!(
            report
                .outcomes
                .iter()
                .all(|o| !o.skipped && o.deleted >= 1),
            "expected deletions: {:?}",
            report.outcomes
        );
        assert!(!passwords::user_exists(&pool, "gone@test").await.unwrap());
        assert_eq!(list_inbox(&mailbox, "u@test").await.unwrap().len(), 0);
    }
}
