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

//! systemd unit (Madmail `install.go` `systemdServiceTemplate`).

use std::path::Path;
use std::process::Command;

use chatmail_types::{ChatmailError, Result};

use super::config::InstallConfig;

pub fn install_unit(cfg: &InstallConfig) -> Result<()> {
    let unit_path = Path::new("/etc/systemd/system").join(format!("{}.service", cfg.binary_name));
    // Match Madmail install.go (StateDirectory/ConfigurationDirectory, caps for port 25).
    // No MemoryDenyWriteExecute — breaks some static-pie builds under systemd.
    let body = format!(
        r#"[Unit]
Description=Madmail mail server ({name})
Documentation=https://github.com/themadorg/madmail
After=network-online.target
Wants=network-online.target

[Service]
Type=notify
NotifyAccess=main
TimeoutStartSec=120

User={user}
Group={group}
WorkingDirectory={state_dir}

StateDirectory={name}
ConfigurationDirectory={name}
RuntimeDirectory={name}
LogsDirectory={name}
ReadWritePaths={state_dir} {config_dir}

PrivateTmp=true
ProtectHome=true
ProtectSystem=full
NoNewPrivileges=true

AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE

UMask=0007
LimitNOFILE=131072
LimitNPROC=512

ExecStart={binary} --config {config} run --libexec {state_dir}
Restart=on-failure
TimeoutStopSec=7s
KillMode=mixed
KillSignal=SIGTERM

[Install]
WantedBy=multi-user.target
"#,
        name = cfg.binary_name,
        user = cfg.maddy_user,
        group = cfg.maddy_group,
        state_dir = cfg.state_dir.display(),
        config_dir = cfg.config_path.parent().unwrap_or(Path::new("/etc/madmail")).display(),
        binary = cfg.binary_path.display(),
        config = cfg.config_path.display(),
    );

    std::fs::write(&unit_path, body)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", unit_path.display())))?;
    println!("✓ Wrote {}", unit_path.display());
    Ok(())
}

pub fn daemon_reload() -> Result<()> {
    let status = Command::new("systemctl")
        .arg("daemon-reload")
        .status()
        .map_err(|e| ChatmailError::config(format!("systemctl daemon-reload: {e}")))?;
    if !status.success() {
        return Err(ChatmailError::config(
            "systemctl daemon-reload failed (is systemd available?)",
        ));
    }
    println!("✓ systemctl daemon-reload");
    Ok(())
}
