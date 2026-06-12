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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chatmail_config::{
    effective_app_db_path, effective_database_config, effective_default_quota_bytes, load_config,
    AppConfig, Args,
};
use chatmail_db::{init_db_from_config, DbPool};
use chatmail_state::AppState;
use chatmail_types::Result;
use tracing::info;

use crate::admin::resolve_admin_token;
use crate::logging::{init_logging, set_no_log, should_disable_logging};

/// Result of a successful boot (for unit tests).
#[derive(Debug)]
pub struct BootArtifacts {
    pub state_dir: PathBuf,
    pub db_path: PathBuf,
    pub admin_token: String,
}

/// Load static config when the path exists; otherwise defaults.
pub fn load_file_config(path: &Path) -> Result<AppConfig> {
    if path.exists() {
        load_config(path)
    } else {
        Ok(AppConfig::default())
    }
}

/// Resolve state directory from CLI and optional config overlay.
pub fn resolve_state_dir(args: &Args, config: &AppConfig) -> PathBuf {
    config
        .state_dir
        .clone()
        .unwrap_or_else(|| args.state_dir.clone())
}

/// Core boot: state dir, DB migrate, admin token.
pub async fn initialize_state(
    state_dir: &Path,
    config: &AppConfig,
) -> Result<(BootArtifacts, DbPool)> {
    std::fs::create_dir_all(state_dir)?;
    let database = effective_database_config(state_dir, config);
    let db_path = effective_app_db_path(state_dir, config);
    let pool = init_db_from_config(&database).await?;
    chatmail_push::hydrate_push_stats(&pool).await?;
    chatmail_push::start_push_stats_flush_task(pool.clone());
    // No routine boot logging under No-Log; DB open failures surface via `?` → stderr.
    let admin_token = resolve_admin_token(state_dir, config)?;
    let artifacts = BootArtifacts {
        state_dir: state_dir.to_path_buf(),
        db_path,
        admin_token,
    };
    Ok((artifacts, pool))
}

/// Full application boot (Phase 2: hydrate caches + background flusher).
pub async fn run(args: Args) -> Result<()> {
    let file_config = load_file_config(&args.config)?;
    let state_dir = resolve_state_dir(&args, &file_config);

    let debug = file_config.debug;
    let log_reload = init_logging(debug);
    if should_disable_logging(file_config.log_target.as_deref(), debug) {
        set_no_log(&log_reload);
    }

    let (artifacts, pool) = initialize_state(&state_dir, &file_config).await?;

    let default_quota = effective_default_quota_bytes(&file_config);
    let app_state = Arc::new(AppState::with_quota_and_message_limit(
        &state_dir,
        default_quota,
        &file_config,
        pool.clone(),
    ));
    app_state.hydrate(&pool, &file_config).await?;
    std::fs::create_dir_all(state_dir.join("pending_notifications"))?;
    app_state.push.requeue_persistent().await;

    let flusher = app_state.start_flusher(pool.clone());

    if debug {
        info!("madmail-v2 starting (Phases 3–8: auth, SMTP, IMAP, federation, delivery)");
    }

    #[cfg(feature = "pprof")]
    {
        // Benchmark builds only: `cargo build -p chatmail --features pprof`
        // Access via: curl 'http://127.0.0.1:6060/debug/pprof/flamegraph?seconds=15'
        crate::profiling::start_pprof_server().await;
    }

    let _supervisor = if !args.boot_once {
        let (supervisor, _reload_tx) = crate::servers::start_servers(
            pool.clone(),
            Arc::clone(&app_state),
            &file_config,
            &state_dir,
            &artifacts.admin_token,
        )
        .await?;
        Some(supervisor)
    } else {
        None
    };

    if args.boot_once {
        flusher.shutdown().await;
        return Ok(());
    }

    tokio::signal::ctrl_c().await?;
    if debug {
        info!("shutdown signal received, flushing federation stats");
    }
    flusher.shutdown().await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn p1_boot_creates_db_and_admin_token() {
        let dir = tempfile::tempdir().unwrap();
        let (artifacts, _pool) = initialize_state(dir.path(), &AppConfig::default())
            .await
            .unwrap();

        assert!(artifacts.db_path.is_file());
        assert!(dir.path().join("admin_token").is_file());
        assert_eq!(artifacts.admin_token.len(), 64);
    }

    #[tokio::test]
    async fn p2_hydrate_quota_and_maildir() {
        let dir = tempfile::tempdir().unwrap();
        let (artifacts, pool) = initialize_state(dir.path(), &AppConfig::default())
            .await
            .unwrap();
        let config = AppConfig::default();
        let app = AppState::new(artifacts.state_dir.clone(), pool.clone());
        app.hydrate(&pool, &config).await.unwrap();

        let store = &app.mailbox_store;
        chatmail_storage::write_blob(store, "u@x.org", "m1", b"hello")
            .await
            .unwrap();
        app.quota.record_write("u@x.org", 5);
        app.quota.check_quota("u@x.org", 1).unwrap();
    }

    #[test]
    fn p1_maddy_log_off_disables_tracing() {
        use crate::logging::{logging_enabled, maddy_log_off, should_disable_logging};
        assert!(maddy_log_off(None));
        assert!(maddy_log_off(Some("off")));
        assert!(!maddy_log_off(Some("stderr")));
        assert!(!logging_enabled(None));
        assert!(logging_enabled(Some("stderr")));
        assert!(should_disable_logging(None, false));
        assert!(should_disable_logging(Some("off"), false));
        assert!(!should_disable_logging(Some("stderr"), false));
        assert!(!should_disable_logging(Some("off"), true));
    }
}
