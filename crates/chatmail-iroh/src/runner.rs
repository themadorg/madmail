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

//! Spawn embedded `iroh-relay` subprocess (cmdeploy `iroh-relay.toml` parity).

use std::net::{Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};

/// Options when starting the relay.
#[derive(Debug, Clone)]
pub struct IrohSpawnOpts {
    pub listen: SocketAddr,
    /// Written to `enable_stun` in config (cmdeploy disables STUN when TURN is separate).
    pub enable_stun: bool,
}

impl Default for IrohSpawnOpts {
    fn default() -> Self {
        Self {
            listen: SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 3340),
            enable_stun: false,
        }
    }
}

/// Running iroh-relay child (kept alive until dropped).
pub struct IrohRelayHandle {
    child: Child,
    pub listen: SocketAddr,
    pub config_path: PathBuf,
}

/// Path to embedded binary (set by `build.rs` when assets exist).
pub fn embedded_binary_path() -> Option<&'static str> {
    option_env!("CHATMAIL_IROH_RELAY_PATH")
}

/// Embedded release tag (e.g. `v0.35.0`).
pub fn embedded_version() -> Option<&'static str> {
    option_env!("CHATMAIL_IROH_RELAY_VERSION")
}

/// Resolve binary: `CHATMAIL_IROH_RELAY_PATH` env, then compile-time embed, then `iroh-relay` on PATH.
pub fn resolve_binary() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("CHATMAIL_IROH_RELAY_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    if let Some(p) = embedded_binary_path() {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    Ok(PathBuf::from("iroh-relay"))
}

/// Write config and spawn `iroh-relay --config-path`.
pub async fn spawn_iroh_relay(
    state_dir: &Path,
    opts: IrohSpawnOpts,
) -> Result<IrohRelayHandle> {
    let binary = resolve_binary()?;
    let config_path = state_dir.join("iroh-relay.toml");
    write_config(&config_path, &opts).await?;

    let mut cmd = Command::new(&binary);
    cmd.arg("--config-path").arg(&config_path);
    cmd.kill_on_drop(true);
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::piped());

    let child = cmd
        .spawn()
        .with_context(|| format!("spawn iroh-relay ({})", binary.display()))?;

    tracing::info!(
        binary = %binary.display(),
        listen = %opts.listen,
        version = embedded_version().unwrap_or("unknown"),
        "iroh-relay started"
    );

    Ok(IrohRelayHandle {
        child,
        listen: opts.listen,
        config_path,
    })
}

async fn write_config(path: &Path, opts: &IrohSpawnOpts) -> Result<()> {
    let bind = format!("[{}]:{}", opts.listen.ip(), opts.listen.port());
    let body = format!(
        r#"enable_relay = true
http_bind_addr = "{bind}"

# cmdeploy / madmail: dedicated TURN handles STUN; iroh-relay 0.35 optional STUN off
enable_stun = {enable_stun}

enable_metrics = false
metrics_bind_addr = "127.0.0.1:9092"
"#,
        enable_stun = if opts.enable_stun { "true" } else { "false" },
    );
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    let mut f = tokio::fs::File::create(path)
        .await
        .with_context(|| format!("write {}", path.display()))?;
    f.write_all(body.as_bytes()).await?;
    f.flush().await?;
    Ok(())
}

impl IrohRelayHandle {
    /// Stop the relay and release the listen port (used on admin disable / soft reload).
    pub async fn shutdown(mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

impl Drop for IrohRelayHandle {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}
