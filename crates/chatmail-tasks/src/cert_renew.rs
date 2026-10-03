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

//! Hook for in-process Let's Encrypt renewal (HTTP-01 needs port 80).

use async_trait::async_trait;
use chatmail_types::Result;

/// Result of an automatic certificate renewal attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertRenewOutcome {
    pub renewed: bool,
    pub skipped: bool,
    pub detail: Option<String>,
}

impl CertRenewOutcome {
    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            renewed: false,
            skipped: true,
            detail: Some(reason.into()),
        }
    }

    pub fn renewed(detail: impl Into<String>) -> Self {
        Self {
            renewed: true,
            skipped: false,
            detail: Some(detail.into()),
        }
    }
}

/// Implemented by the running server supervisor (stops port 80, renews, reloads TLS).
// `#[async_trait]` expansion trips `double_must_use` on newer clippy.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait CertificateRenewer: Send + Sync {
    async fn renew_if_needed(&self) -> Result<CertRenewOutcome>;
}
