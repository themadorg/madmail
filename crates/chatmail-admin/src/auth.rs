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

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use subtle::ConstantTimeEq;

const MAX_FAILED_PER_MINUTE: usize = 10;

pub struct AuthGate {
    token: String,
    failed: Mutex<HashMap<String, Vec<Instant>>>,
}

impl AuthGate {
    pub fn new(token: String) -> Self {
        Self {
            token,
            failed: Mutex::new(HashMap::new()),
        }
    }

    pub fn authenticate(&self, headers: &std::collections::HashMap<String, String>, remote_ip: &str) -> bool {
        if self.token.is_empty() {
            return false;
        }
        let auth = headers
            .get("Authorization")
            .or_else(|| headers.get("authorization"))
            .map(|s| s.as_str())
            .unwrap_or("");
        let Some(token) = auth.strip_prefix("Bearer ") else {
            self.record_failure(remote_ip);
            return false;
        };
        if !self.check_rate_limit(remote_ip) {
            return false;
        }
        if self.token.as_bytes().ct_eq(token.trim().as_bytes()).into() {
            self.clear_failures(remote_ip);
            true
        } else {
            self.record_failure(remote_ip);
            false
        }
    }

    fn check_rate_limit(&self, ip: &str) -> bool {
        let mut map = self.failed.lock().expect("auth lock");
        let cutoff = Instant::now() - Duration::from_secs(60);
        let attempts = map.entry(ip.to_string()).or_default();
        attempts.retain(|t| *t > cutoff);
        attempts.len() < MAX_FAILED_PER_MINUTE
    }

    fn record_failure(&self, ip: &str) {
        let mut map = self.failed.lock().expect("auth lock");
        map.entry(ip.to_string()).or_default().push(Instant::now());
    }

    fn clear_failures(&self, ip: &str) {
        let mut map = self.failed.lock().expect("auth lock");
        map.remove(ip);
    }
}

pub fn extract_ip(remote_addr: &str) -> &str {
    remote_addr.rsplit_once(':').map(|(ip, _)| ip.trim_matches(['[', ']'])).unwrap_or(remote_addr)
}
