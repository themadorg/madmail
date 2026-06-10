// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chatmail_types::Result;
use tokio::sync::oneshot;

/// What to remount on admin-triggered reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadScope {
    /// Stop all listeners, hydrate, rebind SMTP/IMAP/HTTP.
    Full,
    /// Rebuild HTTP routers (admin API, admin-web, www) and restart HTTP listeners only.
    HttpRoutes,
}

/// Supervisor reload queue item.
pub struct ReloadRequest {
    pub scope: ReloadScope,
    pub done: Option<oneshot::Sender<Result<()>>>,
}
