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

//! Shadowsocks server lifecycle (Madmail `runShadowsocks` + admin toggles).

use chatmail_config::AppConfig;
use chatmail_db::DbPool;
use chatmail_shadowsocks::{
    resolve_runtime, spawn_shadowsocks_server, ss_runtime_enabled as ss_enabled,
    ShadowsocksHandle, ShadowsocksRuntime,
};
use chatmail_types::Result;

pub use chatmail_shadowsocks::resolve_runtime as ss_resolve_runtime;

/// Whether SS should accept connections (file config + `__SS_ENABLED__`).
pub async fn ss_runtime_enabled(
    pool: &DbPool,
    file_config: &AppConfig,
    mail_domain: &str,
    state_dir: &std::path::Path,
) -> Result<bool> {
    ss_enabled(pool, file_config, mail_domain, state_dir).await
}

/// Start raw TCP Shadowsocks and optional Xray WS/gRPC when configured and enabled.
pub async fn start_shadowsocks_server(
    pool: &DbPool,
    file_config: &AppConfig,
    mail_domain: &str,
    state_dir: &std::path::Path,
) -> Result<Option<ShadowsocksHandle>> {
    if !ss_runtime_enabled(pool, file_config, mail_domain, state_dir).await? {
        tracing::info!("Shadowsocks disabled (not configured or admin toggle)");
        return Ok(None);
    }
    let rt: ShadowsocksRuntime =
        resolve_runtime(pool, file_config, mail_domain, state_dir).await?;
    let handle = spawn_shadowsocks_server(rt).await?;
    Ok(Some(handle))
}
