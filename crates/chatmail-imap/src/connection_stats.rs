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

//! Live IMAP connection counters for `/admin/status` (more reliable than `ss` alone).

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;

static CONNECTIONS: AtomicI32 = AtomicI32::new(0);
static PEER_IPS: Mutex<Option<HashMap<String, u32>>> = Mutex::new(None);

fn peer_ips() -> std::sync::MutexGuard<'static, Option<HashMap<String, u32>>> {
    let mut guard = PEER_IPS.lock().expect("imap peer_ips lock");
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard
}

/// Call when an IMAP TCP session starts.
pub fn on_open(peer_ip: &str) {
    CONNECTIONS.fetch_add(1, Ordering::Relaxed);
    let mut guard = peer_ips();
    if let Some(map) = guard.as_mut() {
        *map.entry(peer_ip.to_string()).or_insert(0) += 1;
    }
}

/// Call when an IMAP TCP session ends.
pub fn on_close(peer_ip: &str) {
    let prev = CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
    if prev <= 0 {
        CONNECTIONS.store(0, Ordering::Relaxed);
    }
    let mut guard = peer_ips();
    if let Some(map) = guard.as_mut() {
        if let Some(n) = map.get_mut(peer_ip) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                map.remove(peer_ip);
            }
        }
    }
}

/// `(connections, unique_ips)` for admin status.
pub fn snapshot() -> (i32, i32) {
    let (conns, peers) = snapshot_peers();
    (conns, peers.len() as i32)
}

/// Live session count and distinct client IPs (in-process, survives without `ss`).
pub fn snapshot_peers() -> (i32, std::collections::HashSet<String>) {
    let conns = CONNECTIONS.load(Ordering::Relaxed).max(0);
    let guard = peer_ips();
    let peers = guard
        .as_ref()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    (conns, peers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_open_and_close() {
        on_open("1.2.3.4");
        on_open("1.2.3.4");
        on_open("5.6.7.8");
        let (c, u) = snapshot();
        assert_eq!(c, 3);
        assert_eq!(u, 2);
        on_close("1.2.3.4");
        on_close("1.2.3.4");
        on_close("5.6.7.8");
        assert_eq!(snapshot(), (0, 0));
    }
}
