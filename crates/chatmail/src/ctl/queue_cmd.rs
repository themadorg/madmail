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

//! `madmail queue` — inspect and purge the outbound delivery queue on disk.
//!
//! Queue entries live under `{state_dir}/remote_queue` (or `target.queue` location).
//! This is the federation/SMTP retry store, not IMAP maildir storage
//! (see admin API `/admin/queue` purge_* for mail blobs).

use chatmail_config::{Args, QueueCommand};
use chatmail_delivery::{QueueConfig, QueueMeta, QueueStore};
use chatmail_types::{ChatmailError, Result};
use serde_json::json;

use super::context::CtlContext;
use super::output::CtlOut;
use super::util::confirm;

fn open_store(ctx: &CtlContext) -> QueueStore {
    let location = ctx.config.queue.effective_location(&ctx.state_dir);
    QueueStore::new(location)
}

fn queue_config(ctx: &CtlContext) -> QueueConfig {
    QueueConfig::from_settings(&ctx.state_dir, &ctx.config.queue)
}

fn meta_json(meta: &QueueMeta) -> serde_json::Value {
    json!({
        "id": meta.id,
        "mail_from": meta.mail_from,
        "rcpt_to": meta.rcpt_to,
        "tries_count": meta.tries_count,
        "queued_at_unix": meta.effective_queued_at(),
        "last_attempt_unix": meta.last_attempt_unix,
        "next_attempt_unix": meta.next_attempt_unix,
        "last_error": meta.last_error,
    })
}

async fn collect_entries(store: &QueueStore) -> Result<Vec<QueueMeta>> {
    let mut ids = store.list_ids().await?;
    ids.sort();
    let mut entries = Vec::with_capacity(ids.len());
    for id in ids {
        match store.read_meta(&id).await {
            Ok(meta) => entries.push(meta),
            Err(e) => {
                tracing::warn!(id = %id, error = %e, "queue: skip unreadable meta");
            }
        }
    }
    Ok(entries)
}

pub async fn queue_cmd(args: &Args, cmd: Option<&QueueCommand>) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let store = open_store(&ctx);
    let qcfg = queue_config(&ctx);

    match cmd {
        None | Some(QueueCommand::Status) => {
            let out = CtlOut::from_args(args, "queue status");
            let count = store.count_entries().await?;
            let path = store.location().display().to_string();
            if out.is_json() {
                return out.emit(json!({
                    "path": path,
                    "count": count,
                    "max_tries": qcfg.max_tries,
                    "max_parallelism": qcfg.max_parallelism,
                    "initial_retry_secs": qcfg.initial_retry.as_secs(),
                    "retry_time_scale": qcfg.retry_time_scale,
                    "max_delivery_secs": qcfg.max_delivery_time.as_secs(),
                    "post_init_delay_secs": qcfg.post_init_delay.as_secs(),
                }));
            }
            out.blank();
            out.line("Outbound delivery queue (federation / remote SMTP retries)");
            out.line(format!("  Path:              {path}"));
            out.line(format!("  Pending entries:   {count}"));
            out.line(format!("  Max tries:         {}", qcfg.max_tries));
            out.line(format!("  Max parallelism:   {}", qcfg.max_parallelism));
            out.line(format!(
                "  Initial retry:     {}s (scale {})",
                qcfg.initial_retry.as_secs(),
                qcfg.retry_time_scale
            ));
            out.line(format!(
                "  Max delivery age:  {}s",
                qcfg.max_delivery_time.as_secs()
            ));
            out.blank();
            out.line("  List entries:  madmail queue list");
            out.line("  Purge all:     madmail queue purge -y");
            out.blank();
            Ok(())
        }
        Some(QueueCommand::List) => {
            let out = CtlOut::from_args(args, "queue list");
            let entries = collect_entries(&store).await?;
            if out.is_json() {
                let list: Vec<_> = entries.iter().map(meta_json).collect();
                return out.emit(json!({
                    "path": store.location().display().to_string(),
                    "count": list.len(),
                    "entries": list,
                }));
            }
            if entries.is_empty() {
                out.line("No pending outbound queue entries.");
                return Ok(());
            }
            out.line(format!(
                "ID\tFROM\tTO\tTRIES\tNEXT_ATTEMPT\tLAST_ERROR  ({} entries)",
                entries.len()
            ));
            for m in entries {
                out.line(format!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    m.id,
                    m.mail_from,
                    m.rcpt_to,
                    m.tries_count,
                    m.next_attempt_unix,
                    m.last_error.as_deref().unwrap_or(""),
                ));
            }
            Ok(())
        }
        Some(QueueCommand::Show { id }) => {
            let out = CtlOut::from_args(args, "queue show");
            let meta = store.read_meta(id).await.map_err(|e| {
                ChatmailError::config(format!("queue entry {id:?} not found or unreadable: {e}"))
            })?;
            let body_path = store.location().join(format!("{id}.body"));
            let body_bytes = tokio::fs::metadata(&body_path)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            if out.is_json() {
                let mut data = meta_json(&meta);
                if let Some(obj) = data.as_object_mut() {
                    obj.insert("body_bytes".into(), json!(body_bytes));
                    obj.insert("path".into(), json!(store.location().display().to_string()));
                }
                return out.emit(data);
            }
            out.line(format!("Id:               {}", meta.id));
            out.line(format!("Mail from:        {}", meta.mail_from));
            out.line(format!("Rcpt to:          {}", meta.rcpt_to));
            out.line(format!("Tries:            {}", meta.tries_count));
            out.line(format!("Queued at (unix): {}", meta.effective_queued_at()));
            out.line(format!("Last attempt:     {}", meta.last_attempt_unix));
            out.line(format!("Next attempt:     {}", meta.next_attempt_unix));
            out.line(format!(
                "Last error:       {}",
                meta.last_error.as_deref().unwrap_or("(none)")
            ));
            out.line(format!("Body size:        {body_bytes} bytes"));
            Ok(())
        }
        Some(QueueCommand::Remove { id, yes }) => {
            let out = CtlOut::from_args(args, "queue remove");
            // Ensure entry exists before prompting.
            let meta = store.read_meta(id).await.map_err(|e| {
                ChatmailError::config(format!("queue entry {id:?} not found or unreadable: {e}"))
            })?;
            if !confirm(
                &format!(
                    "Remove queue entry {} ({} → {})?",
                    meta.id, meta.mail_from, meta.rcpt_to
                ),
                *yes,
            )? {
                return out.aborted();
            }
            store.remove(id).await;
            out.done_msg(
                format!("Removed queue entry {id}"),
                json!({
                    "id": id,
                    "mail_from": meta.mail_from,
                    "rcpt_to": meta.rcpt_to,
                    "deleted": 1,
                }),
                format!("Removed queue entry {id}"),
            )
        }
        Some(QueueCommand::Purge { yes }) => {
            let out = CtlOut::from_args(args, "queue purge");
            let count = store.count_entries().await?;
            if count == 0 {
                if out.is_json() {
                    return out.emit(json!({
                        "path": store.location().display().to_string(),
                        "deleted": 0,
                        "message": "queue already empty",
                    }));
                }
                out.line("Queue is already empty.");
                return Ok(());
            }
            if !confirm(
                &format!(
                    "Purge all {count} outbound queue entries under {}?",
                    store.location().display()
                ),
                *yes,
            )? {
                return out.aborted();
            }
            let deleted = store.purge_all().await?;
            out.done_msg(
                format!("Purged {deleted} outbound queue entries"),
                json!({
                    "path": store.location().display().to_string(),
                    "deleted": deleted,
                }),
                format!("Purged {deleted} queue entries"),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctl::test_harness::{parse_cli, setup_ctl_env};
    use chatmail_config::Command;
    use chatmail_delivery::QueueStore;

    #[tokio::test]
    async fn queue_status_empty() {
        let (dir, args, _db, _pool) = setup_ctl_env().await;
        let _ = dir;
        queue_cmd(&args, None).await.unwrap();
        queue_cmd(&args, Some(&QueueCommand::Status)).await.unwrap();
    }

    #[tokio::test]
    async fn queue_list_show_remove_purge_roundtrip() {
        let (dir, args, _db, _pool) = setup_ctl_env().await;
        let store = QueueStore::new(dir.path().join("remote_queue"));
        store.ensure_dir().await.unwrap();
        store
            .write_new(
                "entry-1",
                "a@test",
                "b@remote.test",
                b"From: a\r\n\r\nbody",
                1_700_000_000,
            )
            .await
            .unwrap();
        store
            .write_new(
                "entry-2",
                "a@test",
                "c@remote.test",
                b"From: a\r\n\r\nbody2",
                1_700_000_001,
            )
            .await
            .unwrap();

        assert_eq!(store.count_entries().await.unwrap(), 2);

        queue_cmd(&args, Some(&QueueCommand::List)).await.unwrap();
        queue_cmd(
            &args,
            Some(&QueueCommand::Show {
                id: "entry-1".into(),
            }),
        )
        .await
        .unwrap();

        queue_cmd(
            &args,
            Some(&QueueCommand::Remove {
                id: "entry-1".into(),
                yes: true,
            }),
        )
        .await
        .unwrap();
        assert_eq!(store.count_entries().await.unwrap(), 1);

        queue_cmd(&args, Some(&QueueCommand::Purge { yes: true }))
            .await
            .unwrap();
        assert_eq!(store.count_entries().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn queue_show_missing_errors() {
        let (dir, args, _db, _pool) = setup_ctl_env().await;
        let _ = dir;
        let err = queue_cmd(&args, Some(&QueueCommand::Show { id: "nope".into() }))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("not found") || err.to_string().contains("nope"),
            "got: {err}"
        );
    }

    #[test]
    fn clap_parses_queue_subcommands() {
        let dir = tempfile::tempdir().unwrap();
        let cli = parse_cli(dir.path(), &["queue"]);
        assert!(matches!(cli.command, Some(Command::Queue { cmd: None })));
        let cli = parse_cli(dir.path(), &["queue", "list"]);
        assert!(matches!(
            cli.command,
            Some(Command::Queue {
                cmd: Some(QueueCommand::List)
            })
        ));
        let cli = parse_cli(dir.path(), &["queue", "purge", "-y"]);
        assert!(matches!(
            cli.command,
            Some(Command::Queue {
                cmd: Some(QueueCommand::Purge { yes: true })
            })
        ));
        let cli = parse_cli(dir.path(), &["queue", "remove", "abc", "--yes"]);
        assert!(matches!(
            cli.command,
            Some(Command::Queue {
                cmd: Some(QueueCommand::Remove { id, yes: true })
            }) if id == "abc"
        ));
        let cli = parse_cli(dir.path(), &["queue", "show", "xyz"]);
        assert!(matches!(
            cli.command,
            Some(Command::Queue {
                cmd: Some(QueueCommand::Show { id })
            }) if id == "xyz"
        ));
    }
}
