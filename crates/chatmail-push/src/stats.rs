// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Successful Delta Chat push delivery counter (persisted in `message_stats`).

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::OnceLock;

use chatmail_types::Result;

use chatmail_db::{db_execute, db_fetch_all, DbPool};

const STAT_NAME: &str = "push_successful_notifications";

static SUCCESSFUL: AtomicI64 = AtomicI64::new(0);
static FLUSH_TASK: OnceLock<()> = OnceLock::new();

pub fn record_successful_delivery() {
    SUCCESSFUL.fetch_add(1, Ordering::Relaxed);
}

pub fn snapshot() -> i64 {
    SUCCESSFUL.load(Ordering::Relaxed)
}

pub async fn hydrate(pool: &DbPool) -> Result<()> {
    let rows: Vec<(String, i64)> =
        db_fetch_all!(pool, (String, i64), "SELECT name, count FROM message_stats")?;
    for (name, count) in rows {
        if name == STAT_NAME {
            SUCCESSFUL.store(count, Ordering::Relaxed);
            break;
        }
    }
    Ok(())
}

pub fn start_flush_task(pool: DbPool) {
    if FLUSH_TASK.set(()).is_ok() {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            tick.tick().await;
            loop {
                tick.tick().await;
                if let Err(e) = flush(&pool).await {
                    tracing::warn!(error = %e, "push stats flush failed");
                }
            }
        });
    }
}

async fn flush(pool: &DbPool) -> Result<()> {
    let count = snapshot();
    db_execute!(
        pool,
        "INSERT INTO message_stats (name, count) VALUES (?, ?)
         ON CONFLICT(name) DO UPDATE SET count = excluded.count",
        STAT_NAME,
        count
    )?;
    Ok(())
}

#[cfg(test)]
pub(crate) async fn test_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    LOCK.lock().await
}

#[cfg(test)]
pub(crate) fn reset_for_test() {
    SUCCESSFUL.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_db::db_fetch_one;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn counters_increment_and_flush() {
        let _guard = test_lock().await;
        let pool = chatmail_db::init_memory_db().await.unwrap();
        reset_for_test();

        record_successful_delivery();
        record_successful_delivery();
        flush(&pool).await.unwrap();

        let row: (i64,) = db_fetch_one!(
            pool,
            (i64,),
            "SELECT count FROM message_stats WHERE name = ?",
            STAT_NAME
        )
        .unwrap();
        assert_eq!(row.0, 2);

        SUCCESSFUL.store(0, Ordering::Relaxed);
        hydrate(&pool).await.unwrap();
        assert_eq!(snapshot(), 2);
    }
}
