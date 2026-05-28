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

use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct NewMessageEvent {
    pub username: String,
    pub msg_id: String,
}

#[derive(Debug, Clone)]
pub struct EventBus {
    tx: broadcast::Sender<NewMessageEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }

    pub fn notify_new_message(&self, username: &str, msg_id: &str) {
        let _ = self.tx.send(NewMessageEvent {
            username: username.to_string(),
            msg_id: msg_id.to_string(),
        });
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NewMessageEvent> {
        self.tx.subscribe()
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
        let mut rx = bus.subscribe();
        bus.notify_new_message("alice@example.org", "msg-42");
        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.username, "alice@example.org");
        assert_eq!(ev.msg_id, "msg-42");
    }

    #[tokio::test]
    async fn p6_ut01_test_event_bus_multiple_subscribers() {
        let bus = EventBus::new();
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        bus.notify_new_message("alice@example.org", "m1");
        assert_eq!(a.recv().await.unwrap().msg_id, "m1");
        assert_eq!(b.recv().await.unwrap().msg_id, "m1");
    }
}
