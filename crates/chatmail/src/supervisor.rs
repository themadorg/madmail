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

use axum::Router;
use chatmail_config::{
    effective_http_plain_listen, effective_http_tls_listen, effective_imap_plain_listen,
    effective_imap_tls_listen, effective_smtp_listen, effective_submission_plain_listen,
    effective_submission_tls_listen, effective_tls_pem_paths, AppConfig,
};
use chatmail_db::{load_mail_port_overrides, DbPool};
use chatmail_delivery::{start_outbound_queue, DeliveryContext};
use chatmail_fed::run_http_listener;
use chatmail_imap::run_imap_listener;
use chatmail_smtp::run_smtp_listener;
use chatmail_state::AppState;
use chatmail_tls::load_server_config;
use chatmail_tasks::MaintenanceHandle;
use chatmail_types::Result;
use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::logging::boot_error;
use crate::servers::{build_http_extra, extend_dev_local_aliases};

use chatmail_imap::ImapSessionConfig;
use chatmail_smtp::SmtpSessionConfig;
use chatmail_iroh::IrohRelayHandle;
use chatmail_shadowsocks::ShadowsocksHandle;
use chatmail_turn::TurnServerHandle;

struct ListenerSlot {
    cancel: CancellationToken,
    join: JoinHandle<()>,
}

struct ActiveListeners {
    smtp: ListenerSlot,
    submission_plain: Option<ListenerSlot>,
    submission_tls: Option<ListenerSlot>,
    imap_plain: Option<ListenerSlot>,
    imap_tls: Option<ListenerSlot>,
    http_plain: Option<ListenerSlot>,
    http_tls: Option<ListenerSlot>,
    openmetrics: Option<ListenerSlot>,
}

struct ResolvedAddrs {
    smtp: String,
    submission_plain: Option<String>,
    submission_tls: Option<String>,
    imap_plain: Option<String>,
    imap_tls: Option<String>,
    http_plain: Option<String>,
    http_tls: Option<String>,
}

struct SupervisorInner {
    pool: DbPool,
    app: Arc<AppState>,
    file_config: AppConfig,
    state_dir: PathBuf,
    smtp_cfg: SmtpSessionConfig,
    submission_cfg: SmtpSessionConfig,
    primary_domain: String,
    local_domains: Vec<String>,
    http_extra: Mutex<Option<Router>>,
    bootstrap_admin_token: String,
    listeners: Mutex<Option<ActiveListeners>>,
    reload_tx: mpsc::Sender<()>,
    turn_server: Mutex<Option<TurnServerHandle>>,
    iroh_relay: Mutex<Option<IrohRelayHandle>>,
    ss_server: Mutex<Option<ShadowsocksHandle>>,
    imap_cfg: Mutex<ImapSessionConfig>,
    maintenance: Mutex<Option<MaintenanceHandle>>,
}

/// Owns SMTP/IMAP/HTTP listeners and applies `POST /admin/reload` (stop, hydrate, rebind).
pub struct ServerSupervisor {
    inner: Arc<SupervisorInner>,
}

impl Drop for ServerSupervisor {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.inner.maintenance.try_lock() {
            if let Some(handle) = guard.take() {
                handle.shutdown();
            }
        }
    }
}

impl ServerSupervisor {
    pub async fn start(
        pool: DbPool,
        app: Arc<AppState>,
        file_config: &AppConfig,
        state_dir: &std::path::Path,
        admin_token: &str,
    ) -> Result<(Self, mpsc::Sender<()>)> {
        let (reload_tx, mut reload_rx) = mpsc::channel(1);

        let hostname = file_config
            .hostname
            .clone()
            .unwrap_or_else(|| "127.0.0.1".into());
        let primary_domain = file_config.effective_registration_domain(Some(&hostname));
        let mut local_domains = file_config.effective_local_domains(&hostname);
        extend_dev_local_aliases(&mut local_domains);
        let jit_domain = file_config.effective_jit_domain(&primary_domain);

        let delivery = DeliveryContext {
            pool: pool.clone(),
            state: Arc::clone(&app),
            primary_domain: primary_domain.clone(),
            local_domains: local_domains.clone(),
        };
        let queue = start_outbound_queue(delivery, state_dir, &file_config.queue).await?;
        if file_config.debug {
            info!(
                path = %queue.store_location().display(),
                max_tries = queue.config().max_tries,
                max_parallelism = queue.config().max_parallelism,
                "outbound retry queue started"
            );
        }

        let credential_policy = file_config.credential_policy();
        let smtp_cfg = SmtpSessionConfig {
            hostname: hostname.clone(),
            primary_domain: primary_domain.clone(),
            local_domains: local_domains.clone(),
            jit_domain: jit_domain.clone(),
            credential_policy,
            require_auth: false,
            module: "smtp",
        };
        let submission_cfg = SmtpSessionConfig {
            hostname: hostname.clone(),
            primary_domain: primary_domain.clone(),
            local_domains: local_domains.clone(),
            jit_domain,
            credential_policy,
            require_auth: true,
            module: "submission",
        };
        let pool_turn = pool.clone();
        let turn_server =
            crate::turn_boot::start_turn_server(&pool_turn, file_config, &hostname).await?;
        let turn_discovery =
            crate::turn_boot::turn_discovery(&pool_turn, file_config, &hostname).await?;
        let iroh_discovery =
            crate::iroh_boot::iroh_discovery(&pool_turn, file_config, &hostname).await?;
        let iroh_relay = crate::iroh_boot::start_iroh_relay(
            &pool_turn,
            file_config,
            state_dir,
            &hostname,
        )
        .await?;
        let ss_server = crate::ss_boot::start_shadowsocks_server(
            &pool_turn,
            file_config,
            &primary_domain,
            state_dir,
        )
        .await?;
        let imap_cfg = ImapSessionConfig {
            hostname: hostname.clone(),
            primary_domain: primary_domain.clone(),
            jit_domain: submission_cfg.jit_domain.clone(),
            credential_policy,
            turn: turn_discovery,
            iroh: iroh_discovery,
        };

        let maintenance = chatmail_tasks::spawn_maintenance_scheduler(
            pool.clone(),
            state_dir,
            file_config,
        );

        let inner = Arc::new(SupervisorInner {
            pool,
            app,
            file_config: file_config.clone(),
            state_dir: state_dir.to_path_buf(),
            smtp_cfg,
            submission_cfg,
            imap_cfg: Mutex::new(imap_cfg),
            primary_domain,
            local_domains,
            http_extra: Mutex::new(None),
            bootstrap_admin_token: admin_token.to_string(),
            listeners: Mutex::new(None),
            reload_tx: reload_tx.clone(),
            turn_server: Mutex::new(turn_server),
            iroh_relay: Mutex::new(iroh_relay),
            ss_server: Mutex::new(ss_server),
            maintenance: Mutex::new(Some(maintenance)),
        });

        inner.rebuild_http_routers().await?;
        inner.start_listeners().await?;
        inner.start_openmetrics().await?;

        #[cfg(unix)]
        {
            if sd_notify::notify(true, &[sd_notify::NotifyState::Ready]).is_err() {
                tracing::debug!("systemd NOTIFY_SOCKET not set or notify failed");
            }
        }

        let bg = Arc::clone(&inner);
        tokio::spawn(async move {
            while reload_rx.recv().await.is_some() {
                let _ = bg.soft_reload().await;
            }
        });

        Ok((Self { inner }, reload_tx))
    }

    pub fn reload_sender(&self) -> mpsc::Sender<()> {
        self.inner.reload_tx.clone()
    }
}

impl SupervisorInner {
    async fn resolve_addrs(&self) -> Result<ResolvedAddrs> {
        let db_ports = load_mail_port_overrides(&self.pool).await?;
        Ok(ResolvedAddrs {
            smtp: std::env::var("CHATMAIL_SMTP_ADDR")
                .ok()
                .unwrap_or_else(|| effective_smtp_listen(&self.file_config, &db_ports)),
            submission_plain: effective_submission_plain_listen(&self.file_config, &db_ports),
            submission_tls: effective_submission_tls_listen(&self.file_config, &db_ports),
            imap_plain: effective_imap_plain_listen(&self.file_config, &db_ports),
            imap_tls: effective_imap_tls_listen(&self.file_config, &db_ports),
            http_plain: effective_http_plain_listen(&self.file_config, &db_ports),
            http_tls: effective_http_tls_listen(&self.file_config, &db_ports),
        })
    }

    fn load_tls_config(
        &self,
        addrs: &ResolvedAddrs,
    ) -> Result<Option<Arc<ServerConfig>>> {
        let needs_tls = addrs.imap_tls.is_some()
            || addrs.submission_tls.is_some()
            || addrs.http_tls.is_some();
        if !needs_tls {
            return Ok(None);
        }
        let (cert, key) = effective_tls_pem_paths(&self.file_config, &self.state_dir);
        Ok(Some(load_server_config(&cert, &key)?))
    }

    async fn start_listeners(&self) -> Result<()> {
        let addrs = self.resolve_addrs().await?;
        let tls_config = self.load_tls_config(&addrs)?;
        let imap_cfg = self.imap_cfg.lock().await.clone();

        self.app.listener_ports.set_runtime(
            &addrs.smtp,
            addrs.imap_plain.clone(),
            addrs.imap_tls.clone(),
            addrs.submission_plain.clone(),
            addrs.submission_tls.clone(),
            addrs.http_plain.clone(),
            addrs.http_tls.clone(),
        );

        if let Some(ref p) = self.file_config.smtp_listen {
            let db_ports = load_mail_port_overrides(&self.pool).await?;
            if let (Some(file_port), Some(db_port)) = (
                chatmail_config::port_from_listen(Some(p.as_str())),
                db_ports.smtp_port.as_deref().filter(|s| !s.is_empty()),
            ) {
                if file_port != db_port && self.file_config.debug {
                    info!(
                        file_smtp_listen = %p,
                        db_smtp_port = %db_port,
                        effective = %addrs.smtp,
                        "SMTP listen: admin DB port overrides config file"
                    );
                }
            }
        }

        if self.file_config.debug {
            info!(
                state_dir = %self.state_dir.display(),
                smtp = %addrs.smtp,
                submission_plain = ?addrs.submission_plain,
                submission_tls = ?addrs.submission_tls,
                imap_plain = ?addrs.imap_plain,
                imap_tls = ?addrs.imap_tls,
                http_plain = ?addrs.http_plain,
                http_tls = ?addrs.http_tls,
                "starting protocol listeners"
            );
        }

        preflight_listen_addrs(
            [
                Some(addrs.smtp.as_str()),
                addrs.submission_plain.as_deref(),
                addrs.submission_tls.as_deref(),
                addrs.imap_plain.as_deref(),
                addrs.imap_tls.as_deref(),
                addrs.http_plain.as_deref(),
                addrs.http_tls.as_deref(),
            ]
            .into_iter()
            .flatten(),
        )
        .await?;

        let smtp_cancel = CancellationToken::new();
        let smtp_join = spawn_smtp(
            addrs.smtp.clone(),
            smtp_cancel.clone(),
            None,
            Arc::clone(&self.app),
            self.pool.clone(),
            self.smtp_cfg.clone(),
        );

        let submission_plain_slot = addrs.submission_plain.map(|addr| {
            let cancel = CancellationToken::new();
            let join = spawn_smtp(
                addr,
                cancel.clone(),
                None,
                Arc::clone(&self.app),
                self.pool.clone(),
                self.submission_cfg.clone(),
            );
            ListenerSlot { cancel, join }
        });

        let submission_tls_slot = addrs.submission_tls.map(|addr| {
            let cancel = CancellationToken::new();
            let tls = tls_config.clone().expect("tls config when submission tls listen set");
            let join = spawn_smtp(
                addr,
                cancel.clone(),
                Some(tls),
                Arc::clone(&self.app),
                self.pool.clone(),
                self.submission_cfg.clone(),
            );
            ListenerSlot { cancel, join }
        });

        let imap_plain_slot = addrs.imap_plain.map(|addr| {
            let cancel = CancellationToken::new();
            let join = spawn_imap(
                addr,
                cancel.clone(),
                None,
                Arc::clone(&self.app),
                self.pool.clone(),
                imap_cfg.clone(),
            );
            ListenerSlot { cancel, join }
        });

        let imap_tls_slot = addrs.imap_tls.map(|addr| {
            let cancel = CancellationToken::new();
            let tls = tls_config.clone().expect("tls config when imap tls listen set");
            let join = spawn_imap(
                addr,
                cancel.clone(),
                Some(tls),
                Arc::clone(&self.app),
                self.pool.clone(),
                imap_cfg.clone(),
            );
            ListenerSlot { cancel, join }
        });

        let http_extra = self.http_extra.lock().await.clone();
        let http_plain_slot = addrs.http_plain.map(|addr| {
            let cancel = CancellationToken::new();
            let join = spawn_http(
                addr,
                cancel.clone(),
                None,
                self.pool.clone(),
                Arc::clone(&self.app),
                self.primary_domain.clone(),
                self.local_domains.clone(),
                http_extra.clone(),
            );
            ListenerSlot { cancel, join }
        });

        let http_tls_slot = addrs.http_tls.map(|addr| {
            let cancel = CancellationToken::new();
            let tls = tls_config.clone().expect("tls config when http tls listen set");
            let join = spawn_http(
                addr,
                cancel.clone(),
                Some(tls),
                self.pool.clone(),
                Arc::clone(&self.app),
                self.primary_domain.clone(),
                self.local_domains.clone(),
                http_extra.clone(),
            );
            ListenerSlot { cancel, join }
        });

        *self.listeners.lock().await = Some(ActiveListeners {
            smtp: ListenerSlot {
                cancel: smtp_cancel,
                join: smtp_join,
            },
            submission_plain: submission_plain_slot,
            submission_tls: submission_tls_slot,
            imap_plain: imap_plain_slot,
            imap_tls: imap_tls_slot,
            http_plain: http_plain_slot,
            http_tls: http_tls_slot,
            openmetrics: None,
        });

        Ok(())
    }

    async fn start_openmetrics(&self) -> Result<()> {
        let Some(addr) = self.file_config.openmetrics_listen.as_deref() else {
            return Ok(());
        };
        preflight_listen_addrs(std::iter::once(addr)).await?;
        let cancel = CancellationToken::new();
        let addr_owned = addr.to_string();
        let cancel_metrics = cancel.clone();
        let metrics_task = async move {
            let _ =
                chatmail_metrics::run_openmetrics_listener(&addr_owned, cancel_metrics).await;
        };
        let cancel_queue = cancel.clone();
        let queue_task = async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
            loop {
                tokio::select! {
                    _ = cancel_queue.cancelled() => break,
                    _ = interval.tick() => {
                        if let Some(q) = chatmail_delivery::outbound_queue() {
                            if let Ok(depth) = q.depth().await {
                                let loc = q.store_location().display().to_string();
                                chatmail_metrics::set_queue_length(
                                    "remote_queue",
                                    &loc,
                                    depth as f64,
                                );
                            }
                        }
                    }
                }
            }
        };
        let join = tokio::spawn(async move {
            tokio::join!(metrics_task, queue_task);
        });

        if let Some(active) = self.listeners.lock().await.as_mut() {
            active.openmetrics = Some(ListenerSlot { cancel, join });
        }

        Ok(())
    }

    async fn stop_listeners(&self) {
        let Some(active) = self.listeners.lock().await.take() else {
            return;
        };
        active.smtp.cancel.cancel();
        cancel_optional(&active.submission_plain);
        cancel_optional(&active.submission_tls);
        cancel_optional(&active.imap_plain);
        cancel_optional(&active.imap_tls);
        cancel_optional(&active.http_plain);
        cancel_optional(&active.http_tls);
        cancel_optional(&active.openmetrics);

        let _ = timeout(Duration::from_secs(8), async {
            let _ = active.smtp.join.await;
            await_optional(active.submission_plain).await;
            await_optional(active.submission_tls).await;
            await_optional(active.imap_plain).await;
            await_optional(active.imap_tls).await;
            await_optional(active.http_plain).await;
            await_optional(active.http_tls).await;
            await_optional(active.openmetrics).await;
        })
        .await;
    }

    async fn soft_reload(&self) -> Result<()> {
        if self.file_config.debug {
            info!("admin soft reload: stopping listeners");
        }
        self.stop_listeners().await;
        self.reload_turn().await?;
        self.reload_iroh().await?;
        self.reload_ss().await?;
        self.app.hydrate(&self.pool, &self.file_config).await?;
        self.rebuild_http_routers().await?;
        if self.file_config.debug {
            info!("admin soft reload: caches hydrated, restarting listeners");
        }
        self.start_listeners().await?;
        self.start_openmetrics().await?;
        Ok(())
    }

    /// Remount admin API, admin-web SPA path, and www routes from current DB settings.
    async fn rebuild_http_routers(&self) -> Result<()> {
        let http_extra = build_http_extra(
            &self.file_config,
            &self.state_dir,
            &self.bootstrap_admin_token,
            self.pool.clone(),
            Arc::clone(&self.app),
            Some(self.reload_tx.clone()),
        )
        .await?;
        *self.http_extra.lock().await = http_extra;
        Ok(())
    }

    /// Apply admin TURN toggle / DB overrides: stop relay, refresh IMAP discovery, maybe restart.
    async fn reload_turn(&self) -> Result<()> {
        let hostname = self.imap_cfg.lock().await.hostname.clone();
        let discovery = crate::turn_boot::turn_discovery(
            &self.pool,
            &self.file_config,
            &hostname,
        )
        .await?;
        {
            let mut imap = self.imap_cfg.lock().await;
            imap.turn = discovery;
        }
        {
            let mut turn = self.turn_server.lock().await;
            *turn = None;
        }
        let started =
            crate::turn_boot::start_turn_server(&self.pool, &self.file_config, &hostname)
                .await?;
        *self.turn_server.lock().await = started;
        Ok(())
    }

    /// Apply admin Iroh toggle / DB overrides: stop relay, refresh IMAP discovery, maybe restart.
    async fn reload_iroh(&self) -> Result<()> {
        let hostname = self.imap_cfg.lock().await.hostname.clone();
        let discovery = crate::iroh_boot::iroh_discovery(
            &self.pool,
            &self.file_config,
            &hostname,
        )
        .await?;
        {
            let mut imap = self.imap_cfg.lock().await;
            imap.iroh = discovery;
        }
        {
            let mut iroh = self.iroh_relay.lock().await;
            if let Some(handle) = iroh.take() {
                handle.shutdown().await;
            }
        }
        let started = crate::iroh_boot::start_iroh_relay(
            &self.pool,
            &self.file_config,
            &self.state_dir,
            &hostname,
        )
        .await?;
        *self.iroh_relay.lock().await = started;
        Ok(())
    }

    async fn reload_ss(&self) -> Result<()> {
        let mail_domain = self.primary_domain.clone();
        {
            let mut ss = self.ss_server.lock().await;
            if let Some(handle) = ss.take() {
                handle.shutdown();
            }
        }
        let started = crate::ss_boot::start_shadowsocks_server(
            &self.pool,
            &self.file_config,
            &mail_domain,
            &self.state_dir,
        )
        .await?;
        *self.ss_server.lock().await = started;
        Ok(())
    }
}

/// Bind each listen address before spawning listeners so port conflicts fail startup visibly.
async fn preflight_listen_addrs(addrs: impl IntoIterator<Item = &str>) -> Result<()> {
    for addr in addrs {
        if let Err(e) = TcpListener::bind(addr).await {
            boot_error(format!("cannot listen on {addr}: {e}"));
            return Err(e.into());
        }
    }
    Ok(())
}

fn cancel_optional(slot: &Option<ListenerSlot>) {
    if let Some(s) = slot {
        s.cancel.cancel();
    }
}

async fn await_optional(slot: Option<ListenerSlot>) {
    if let Some(s) = slot {
        let _ = s.join.await;
    }
}


fn spawn_smtp(
    addr: String,
    cancel: CancellationToken,
    tls: Option<Arc<ServerConfig>>,
    app: Arc<AppState>,
    pool: DbPool,
    cfg: SmtpSessionConfig,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let _ = run_smtp_listener(&addr, cancel, tls, app, pool, cfg).await;
    })
}

fn spawn_imap(
    addr: String,
    cancel: CancellationToken,
    tls: Option<Arc<ServerConfig>>,
    app: Arc<AppState>,
    pool: DbPool,
    cfg: ImapSessionConfig,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let _ = run_imap_listener(&addr, cancel, tls, app, pool, cfg).await;
    })
}

fn spawn_http(
    addr: String,
    cancel: CancellationToken,
    tls: Option<Arc<ServerConfig>>,
    pool: DbPool,
    app: Arc<AppState>,
    primary_domain: String,
    local_domains: Vec<String>,
    http_extra: Option<Router>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let _ = run_http_listener(
            &addr,
            cancel,
            tls,
            pool,
            app,
            primary_domain,
            local_domains,
            http_extra,
        )
        .await;
    })
}
