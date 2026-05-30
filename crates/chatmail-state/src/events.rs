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

use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct NewMessageEvent {
    pub username: String,
    pub msg_id: String,
}

/// Per-user delivery notifications for IMAP IDLE / WebIMAP push.
///
/// Madmail Go used `go-imap-mess.Manager.NewMessage(mboxKey, uid)` — one mailbox at a time.
/// A global broadcast fan-out makes every IDLE session receive every user's events (O(n²)
/// under group delivery bursts).
#[derive(Debug)]
pub struct EventBus {
    users: DashMap<String, broadcast::Sender<NewMessageEvent>>,
    /// Monotonic per-user INBOX change counter. Bumped on every mutation (delivery, append,
    /// flag change, expunge/move). IMAP sessions cache their mailbox listing keyed by this value
    /// so repeated FETCH/STORE/IDLE re-checks within one client cycle skip redundant maildir
    /// scans — the per-command directory walks were the throughput wall under 60-recipient
    /// bursts (delivery itself is ~7ms; the cost was serving 60 clients' IMAP cycles).
    inbox_versions: DashMap<String, AtomicU64>,
    /// Count of notifications whose live push reached no IDLE subscriber (the broadcast had zero
    /// receivers at send time). The `inbox_version` bump still happened so the message is found on
    /// the next listing, but the client missed the instant wakeup. Surfaced for observability so
    /// operators can spot subscription-window losses during bursts instead of them being silent.
    no_receiver_drops: AtomicU64,
}

/// Per-user IDLE notification fan-out buffer.
///
/// `tokio::broadcast` does not block the sender or fail on a full buffer — slow receivers fall
/// behind and observe `Lagged`, after which the IMAP session resyncs via `emit_idle_updates`
/// (correct, but a thundering herd under 60-way media bursts). A larger buffer keeps more bursty
/// notifications in-flight so fewer receivers take the expensive `Lagged` resync path. Send only
/// truly *fails* when there are no receivers at all (tracked via `no_receiver_drops`).
const USER_CHANNEL_CAPACITY: usize = 256;

impl EventBus {
    pub fn new() -> Self {
        Self {
            users: DashMap::new(),
            inbox_versions: DashMap::new(),
            no_receiver_drops: AtomicU64::new(0),
        }
    }

    /// Current INBOX version for `username` (0 if never changed).
    pub fn inbox_version(&self, username: &str) -> u64 {
        let key = username.to_ascii_lowercase();
        self.inbox_versions
            .get(&key)
            .map(|v| v.load(Ordering::Acquire))
            .unwrap_or(0)
    }

    /// Bump the INBOX version after any local mutation that changes the listing.
    pub fn bump_inbox_version(&self, username: &str) {
        let key = username.to_ascii_lowercase();
        self.inbox_versions
            .entry(key)
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::AcqRel);
    }

    fn user_sender(&self, username: &str) -> broadcast::Sender<NewMessageEvent> {
        let key = username.to_ascii_lowercase();
        self.users
            .entry(key)
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(USER_CHANNEL_CAPACITY);
                tx
            })
            .clone()
    }

    pub fn notify_new_message(&self, username: &str, msg_id: &str) {
        // Bump the durable listing version FIRST so the message is discoverable on the next scan
        // even if the live push below reaches no one — notification is best-effort, the version
        // bump is the source of truth.
        self.bump_inbox_version(username);
        let sender = self.user_sender(username);
        let result = sender.send(NewMessageEvent {
            username: username.to_string(),
            msg_id: msg_id.to_string(),
        });
        if result.is_err() {
            // No active IDLE/WebIMAP subscriber for this user right now; the live wakeup is lost
            // but the bumped version means the next SELECT/STATUS/IDLE re-check still surfaces it.
            self.no_receiver_drops.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(
                user = %username,
                msg_id = %msg_id,
                "new-message notification had no live subscriber (recoverable via inbox_version)"
            );
        }
    }

    /// Number of notifications that reached no live subscriber since boot (observability).
    pub fn no_receiver_drops(&self) -> u64 {
        self.no_receiver_drops.load(Ordering::Relaxed)
    }

    /// Count of currently-subscribed IDLE/WebIMAP receivers for `username`.
    pub fn subscriber_count(&self, username: &str) -> usize {
        let key = username.to_ascii_lowercase();
        self.users
            .get(&key)
            .map(|s| s.receiver_count())
            .unwrap_or(0)
    }

    pub fn subscribe(&self, username: &str) -> broadcast::Receiver<NewMessageEvent> {
        self.user_sender(username).subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P6-UT01: IDLE subscribers receive delivery notifications for their user.
    #[tokio::test]
    async fn p6_ut01_test_event_bus_notifies_subscriber() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe("alice@example.org");
        bus.notify_new_message("alice@example.org", "msg-42");
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.username, "alice@example.org");
        assert_eq!(ev.msg_id, "msg-42");
    }

    #[tokio::test]
    async fn p6_ut01_test_event_bus_per_user_isolation() {
        let bus = EventBus::new();
        let mut alice = bus.subscribe("alice@example.org");
        let mut bob = bus.subscribe("bob@example.org");
        bus.notify_new_message("alice@example.org", "m1");
        assert_eq!(alice.recv().await.unwrap().msg_id, "m1");
        bus.notify_new_message("bob@example.org", "m2");
        assert_eq!(bob.recv().await.unwrap().msg_id, "m2");
    }

    /// P10-UT01: a notification with no live subscriber is counted (not silently lost) and the
    /// inbox_version is still bumped so the message remains discoverable.
    #[tokio::test]
    async fn p10_ut01_notify_without_subscriber_is_tracked() {
        let bus = EventBus::new();
        assert_eq!(bus.no_receiver_drops(), 0);
        assert_eq!(bus.subscriber_count("ghost@example.org"), 0);

        let before = bus.inbox_version("ghost@example.org");
        bus.notify_new_message("ghost@example.org", "m1");
        assert_eq!(
            bus.no_receiver_drops(),
            1,
            "drop with no subscriber must be counted"
        );
        assert_eq!(
            bus.inbox_version("ghost@example.org"),
            before + 1,
            "version still bumped so the message is recoverable on next listing"
        );

        // With a live subscriber, the same notification is delivered and not counted as a drop.
        let mut rx = bus.subscribe("ghost@example.org");
        assert_eq!(bus.subscriber_count("ghost@example.org"), 1);
        bus.notify_new_message("ghost@example.org", "m2");
        assert_eq!(rx.recv().await.unwrap().msg_id, "m2");
        assert_eq!(bus.no_receiver_drops(), 1, "no new drop when delivered");
    }

    /// P10-UT01: a slow receiver that overflows the buffer observes `Lagged` (resync signal)
    /// rather than the bus blocking or the sender failing — and later notifications still arrive.
    #[tokio::test]
    async fn p10_ut01_slow_receiver_lags_then_resyncs() {
        use tokio::sync::broadcast::error::{RecvError, TryRecvError};
        let bus = EventBus::new();
        let mut rx = bus.subscribe("busy@example.org");
        for i in 0..(USER_CHANNEL_CAPACITY + 10) {
            bus.notify_new_message("busy@example.org", &format!("m{i}"));
        }
        // The slow receiver observes `Lagged` (its resync signal) rather than the bus blocking or
        // the sender failing — in the real server this drives emit_idle_updates keyed on the
        // bumped inbox_version.
        match rx.recv().await {
            Err(RecvError::Lagged(skipped)) => assert!(skipped >= 10),
            other => panic!("expected Lagged, got {other:?}"),
        }
        // Drain whatever remains, tolerating further lag (the cursor may be evicted again).
        loop {
            match rx.try_recv() {
                Ok(_) => continue,
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
            }
        }
        // Once caught up, subsequent notifications are still delivered live.
        bus.notify_new_message("busy@example.org", "after-lag");
        assert_eq!(rx.recv().await.unwrap().msg_id, "after-lag");
        // No drops: there was always a live subscriber.
        assert_eq!(bus.no_receiver_drops(), 0);
    }
}
