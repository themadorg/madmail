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

//! Global message counters (Madmail `framework/module/msgcounter.go`).

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::OnceLock;

use chatmail_types::Result;

use crate::{db_execute, db_fetch_all, DbPool};

static SENT: AtomicI64 = AtomicI64::new(0);
static OUTBOUND: AtomicI64 = AtomicI64::new(0);
static RECEIVED: AtomicI64 = AtomicI64::new(0);
static FLUSH_TASK: OnceLock<()> = OnceLock::new();

pub fn increment_sent() {
    SENT.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_outbound() {
    OUTBOUND.fetch_add(1, Ordering::Relaxed);
}

pub fn increment_received() {
    RECEIVED.fetch_add(1, Ordering::Relaxed);
}

pub fn record_smtp_accepted(submission: bool) {
    increment_sent();
    if !submission {
        increment_received();
    }
}

pub fn record_inbound_delivery() {
    increment_received();
}

pub fn snapshot() -> (i64, i64, i64) {
    (
        SENT.load(Ordering::Relaxed),
        OUTBOUND.load(Ordering::Relaxed),
        RECEIVED.load(Ordering::Relaxed),
    )
}

pub async fn hydrate(pool: &DbPool) -> Result<()> {
    let rows: Vec<(String, i64)> =
        db_fetch_all!(pool, (String, i64), "SELECT name, count FROM message_stats")?;
    for (name, count) in rows {
        match name.as_str() {
            "sent_messages" => SENT.store(count, Ordering::Relaxed),
            "outbound_messages" => OUTBOUND.store(count, Ordering::Relaxed),
            "received_messages" => RECEIVED.store(count, Ordering::Relaxed),
            _ => {}
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
                    tracing::warn!(error = %e, "message_stats flush failed");
                }
            }
        });
    }
}

async fn flush(pool: &DbPool) -> Result<()> {
    let (sent, outbound, received) = snapshot();
    for (name, count) in [
        ("sent_messages", sent),
        ("outbound_messages", outbound),
        ("received_messages", received),
    ] {
        db_execute!(
            pool,
            "INSERT INTO message_stats (name, count) VALUES (?, ?)
             ON CONFLICT(name) DO UPDATE SET count = excluded.count",
            name,
            count
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db_fetch_one;

    #[tokio::test]
    async fn counters_increment_and_flush() {
        let pool = crate::init_memory_db().await.unwrap();
        SENT.store(0, Ordering::Relaxed);
        OUTBOUND.store(0, Ordering::Relaxed);
        RECEIVED.store(0, Ordering::Relaxed);

        record_smtp_accepted(false);
        increment_outbound();
        flush(&pool).await.unwrap();

        let sent: (i64,) = db_fetch_one!(
            pool,
            (i64,),
            "SELECT count FROM message_stats WHERE name = 'sent_messages'"
        )
        .unwrap();
        assert_eq!(sent.0, 1);

        hydrate(&pool).await.unwrap();
        assert_eq!(snapshot(), (1, 1, 1));
    }
}
