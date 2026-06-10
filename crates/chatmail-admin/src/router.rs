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

use std::path::PathBuf;
use std::sync::Arc;

use axum::middleware;
use axum::routing::post;
use axum::Router;
use chatmail_config::AppConfig;
use chatmail_db::DbPool;
use chatmail_state::{AppState, ReloadRequest};
use tokio::sync::mpsc;

use crate::auth::AuthGate;
use crate::cors::cors_middleware;
use crate::handler::admin_handler;

#[derive(Clone)]
pub struct AdminState {
    pub pool: DbPool,
    pub app: Arc<AppState>,
    pub file_config: AppConfig,
    pub state_dir: PathBuf,
    /// `$(primary_domain)` / registration domain for admin-created accounts.
    pub mail_domain: String,
    pub auth: Arc<AuthGate>,
    pub version: String,
    pub reload_tx: Option<mpsc::Sender<ReloadRequest>>,
}

impl AdminState {
    pub fn new(
        pool: DbPool,
        app: Arc<AppState>,
        file_config: AppConfig,
        state_dir: PathBuf,
        mail_domain: String,
        token: String,
        reload_tx: Option<mpsc::Sender<ReloadRequest>>,
    ) -> Self {
        Self {
            pool,
            app,
            file_config,
            state_dir,
            mail_domain,
            auth: Arc::new(AuthGate::new(token)),
            version: env!("CARGO_PKG_VERSION").to_string(),
            reload_tx,
        }
    }
}

pub fn admin_router(state: AdminState) -> Router {
    Router::new()
        .route("/", post(admin_handler))
        .layer(middleware::from_fn(cors_middleware))
        .with_state(state)
}
