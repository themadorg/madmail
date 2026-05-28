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

use std::sync::atomic::{AtomicI64, Ordering};

use chatmail_types::Result;
use dashmap::DashMap;
use chatmail_db::DbPool;

/// Per-domain federation diagnostics (mirrors Madmail `ServerStat`).
#[derive(Debug)]
pub struct ServerStat {
    pub domain: String,
    pub queued_messages: AtomicI64,
    pub failed_http: AtomicI64,
    pub failed_https: AtomicI64,
    pub failed_smtp: AtomicI64,
    pub success_http: AtomicI64,
    pub success_https: AtomicI64,
    pub success_smtp: AtomicI64,
    pub inbound_deliveries: AtomicI64,
    pub successful_deliveries: AtomicI64,
    pub total_latency_ms: AtomicI64,
    pub last_active: AtomicI64,
}

impl ServerStat {
    fn new(domain: String) -> Self {
        let now = now_unix();
        Self {
            domain,
            queued_messages: AtomicI64::new(0),
            failed_http: AtomicI64::new(0),
            failed_https: AtomicI64::new(0),
            failed_smtp: AtomicI64::new(0),
            success_http: AtomicI64::new(0),
            success_https: AtomicI64::new(0),
            success_smtp: AtomicI64::new(0),
            inbound_deliveries: AtomicI64::new(0),
            successful_deliveries: AtomicI64::new(0),
            total_latency_ms: AtomicI64::new(0),
            last_active: AtomicI64::new(now),
        }
    }

    fn touch(&self) {
        self.last_active.store(now_unix(), Ordering::Relaxed);
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug)]
pub struct FederationTracker {
    stats: DashMap<String, ServerStat>,
}

impl FederationTracker {
    pub fn new() -> Self {
        Self {
            stats: DashMap::new(),
        }
    }

    fn domain_key(domain: &str) -> String {
        domain.to_ascii_lowercase()
    }

    pub fn increment_queue(&self, domain: &str) {
        let key = Self::domain_key(domain);
        let entry = self
            .stats
            .entry(key.clone())
            .or_insert_with(|| ServerStat::new(key));
        entry.queued_messages.fetch_add(1, Ordering::Relaxed);
        entry.touch();
    }

    pub fn decrement_queue(&self, domain: &str) {
        let key = Self::domain_key(domain);
        if let Some(entry) = self.stats.get(&key) {
            let q = entry.queued_messages.load(Ordering::Relaxed);
            if q > 0 {
                entry.queued_messages.fetch_sub(1, Ordering::Relaxed);
            }
            entry.touch();
        }
    }

    pub fn record_failure(&self, domain: &str, transport: &str) {
        let key = Self::domain_key(domain);
        let entry = self
            .stats
            .entry(key.clone())
            .or_insert_with(|| ServerStat::new(key));
        match transport.to_ascii_uppercase().as_str() {
            "HTTP" => {
                entry.failed_http.fetch_add(1, Ordering::Relaxed);
            }
            "HTTPS" => {
                entry.failed_https.fetch_add(1, Ordering::Relaxed);
            }
            "SMTP" => {
                entry.failed_smtp.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
        entry.touch();
    }

    pub fn record_success(&self, domain: &str, latency_ms: i64, transport: &str) {
        let key = Self::domain_key(domain);
        let entry = self
            .stats
            .entry(key.clone())
            .or_insert_with(|| ServerStat::new(key));
        entry
            .successful_deliveries
            .fetch_add(1, Ordering::Relaxed);
        entry
            .total_latency_ms
            .fetch_add(latency_ms.max(0), Ordering::Relaxed);
        if transport.is_empty() {
            entry.inbound_deliveries.fetch_add(1, Ordering::Relaxed);
        } else {
            match transport.to_ascii_uppercase().as_str() {
                "HTTP" => {
                    entry.success_http.fetch_add(1, Ordering::Relaxed);
                }
                "HTTPS" => {
                    entry.success_https.fetch_add(1, Ordering::Relaxed);
                }
                "SMTP" => {
                    entry.success_smtp.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }
        }
        entry.touch();
    }

    /// Load persisted rows from `federation_server_stats` (Madmail `FederationTracker.Hydrate`).
    pub async fn hydrate(&self, pool: &DbPool) -> Result<()> {
        let cols = chatmail_db::schema::federation_stats_columns(pool).await?;
        let sql = format!(
            "SELECT domain, queued_messages, failed_http, {}, failed_smtp,
                    success_http, {}, success_smtp, inbound_deliveries,
                    successful_deliveries, total_latency_ms, last_active
             FROM federation_server_stats",
            cols.failed_https, cols.success_https
        );
        let rows: Vec<(
            String,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
        )> = chatmail_db::db_fetch_all!(
            pool,
            (
                String,
                i64,
                i64,
                i64,
                i64,
                i64,
                i64,
                i64,
                i64,
                i64,
                i64,
                i64
            ),
            &sql
        )?;

        for (
            domain,
            queued_messages,
            failed_http,
            failed_https,
            failed_smtp,
            success_http,
            success_https,
            success_smtp,
            inbound_deliveries,
            successful_deliveries,
            total_latency_ms,
            last_active,
        ) in rows
        {
            let row = FederationStatRow {
                domain,
                queued_messages,
                failed_http,
                failed_https,
                failed_smtp,
                success_http,
                success_https,
                success_smtp,
                inbound_deliveries,
                successful_deliveries,
                total_latency_ms,
                last_active,
            };
            let key = Self::domain_key(&row.domain);
            let entry = self
                .stats
                .entry(key.clone())
                .or_insert_with(|| ServerStat::new(key));
            entry
                .queued_messages
                .store(row.queued_messages, Ordering::Relaxed);
            entry.failed_http.store(row.failed_http, Ordering::Relaxed);
            entry
                .failed_https
                .store(row.failed_https, Ordering::Relaxed);
            entry.failed_smtp.store(row.failed_smtp, Ordering::Relaxed);
            entry.success_http.store(row.success_http, Ordering::Relaxed);
            entry
                .success_https
                .store(row.success_https, Ordering::Relaxed);
            entry.success_smtp.store(row.success_smtp, Ordering::Relaxed);
            entry
                .inbound_deliveries
                .store(row.inbound_deliveries, Ordering::Relaxed);
            entry
                .successful_deliveries
                .store(row.successful_deliveries, Ordering::Relaxed);
            entry
                .total_latency_ms
                .store(row.total_latency_ms, Ordering::Relaxed);
            entry.last_active.store(row.last_active, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Vec<FederationStatRow> {
        self.stats
            .iter()
            .map(|r| FederationStatRow {
                domain: r.key().clone(),
                queued_messages: r.queued_messages.load(Ordering::Relaxed),
                failed_http: r.failed_http.load(Ordering::Relaxed),
                failed_https: r.failed_https.load(Ordering::Relaxed),
                failed_smtp: r.failed_smtp.load(Ordering::Relaxed),
                success_http: r.success_http.load(Ordering::Relaxed),
                success_https: r.success_https.load(Ordering::Relaxed),
                success_smtp: r.success_smtp.load(Ordering::Relaxed),
                inbound_deliveries: r.inbound_deliveries.load(Ordering::Relaxed),
                successful_deliveries: r.successful_deliveries.load(Ordering::Relaxed),
                total_latency_ms: r.total_latency_ms.load(Ordering::Relaxed),
                last_active: r.last_active.load(Ordering::Relaxed),
            })
            .collect()
    }
}

impl Default for FederationTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct FederationStatRow {
    pub domain: String,
    pub queued_messages: i64,
    pub failed_http: i64,
    pub failed_https: i64,
    pub failed_smtp: i64,
    pub success_http: i64,
    pub success_https: i64,
    pub success_smtp: i64,
    pub inbound_deliveries: i64,
    pub successful_deliveries: i64,
    pub total_latency_ms: i64,
    pub last_active: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tracker_hydrate_loads_persisted_stats() {
        let pool = chatmail_db::init_memory_db().await.unwrap();
        chatmail_db::db_execute!(
            pool,
            "INSERT INTO federation_server_stats (
                domain, queued_messages, failed_http, failed_https, failed_smtp,
                success_http, success_https, success_smtp, inbound_deliveries,
                successful_deliveries, total_latency_ms, last_active
             ) VALUES ('peer.example.org', 1, 2, 3, 4, 5, 6, 7, 8, 9, 100, 1700000000)"
        )
        .unwrap();
        let tracker = FederationTracker::new();
        tracker.hydrate(&pool).await.unwrap();
        let snap = tracker.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].domain, "peer.example.org");
        assert_eq!(snap[0].success_https, 6);
    }

    /// P2-UT04: success/failure counters update correctly.
    #[test]
    fn p2_ut04_test_fed_tracker_stats() {
        let t = FederationTracker::new();
        t.increment_queue("Example.ORG");
        t.increment_queue("example.org");
        t.record_success("example.org", 42, "HTTPS");
        t.record_failure("example.org", "SMTP");
        t.decrement_queue("example.org");

        let snap = t.snapshot();
        let stat = snap.iter().find(|s| s.domain == "example.org").unwrap();
        assert_eq!(stat.queued_messages, 1);
        assert_eq!(stat.success_https, 1);
        assert_eq!(stat.failed_smtp, 1);
        assert_eq!(stat.total_latency_ms, 42);
    }
}
