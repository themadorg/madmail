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

//! IMAP METADATA discovery for Delta Chat core (`context/core/src/imap.rs`).

/// Relay URL advertised at `/shared/vendor/deltachat/irohrelay`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrohDiscovery {
    pub relay_url: String,
}

impl IrohDiscovery {
    pub fn enabled(&self) -> bool {
        !self.relay_url.is_empty()
    }

    /// Build from static config / admin overrides.
    pub fn from_relay_url(url: String) -> Option<Self> {
        let url = url.trim().to_string();
        if url.is_empty() {
            return None;
        }
        // Core uses `url::Url::parse`; reject obvious garbage early.
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return None;
        }
        Some(Self { relay_url: url })
    }
}
