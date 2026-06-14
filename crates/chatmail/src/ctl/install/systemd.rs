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
    let body = render_systemd_unit(cfg);

    std::fs::write(&unit_path, body)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", unit_path.display())))?;
    println!("✓ Wrote {}", unit_path.display());
    Ok(())
}

/// Build the `.service` unit body. Default FHS paths use `StateDirectory` /
/// `ConfigurationDirectory`; explicit `--config-dir` / `--state-dir` are reflected
/// in `ExecStart`, `WorkingDirectory`, and `ReadWritePaths` only.
pub fn render_systemd_unit(cfg: &InstallConfig) -> String {
    let managed_dirs = if cfg.use_default_systemd_paths {
        format!(
            r#"
StateDirectory={name}
ConfigurationDirectory={name}
RuntimeDirectory={name}
LogsDirectory={name}"#,
            name = cfg.binary_name,
        )
    } else {
        String::new()
    };

    format!(
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
{managed_dirs}
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
        managed_dirs = managed_dirs,
        config_dir = cfg.config_dir.display(),
        binary = cfg.binary_path.display(),
        config = cfg.config_path.display(),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use super::super::config::InstallConfig;

    fn sample_cfg() -> InstallConfig {
        InstallConfig {
            binary_name: "madmail".into(),
            binary_path: PathBuf::from("/usr/local/bin/madmail"),
            maddy_user: "madmail".into(),
            maddy_group: "madmail".into(),
            hostname: "mail.example.org".into(),
            primary_domain: "mail.example.org".into(),
            local_domains: "$(primary_domain)".into(),
            state_dir: PathBuf::from("/var/lib/madmail"),
            runtime_dir: "/run/madmail".into(),
            public_ip: "203.0.113.1".into(),
            tls_mode: "self_signed".into(),
            cert_path: PathBuf::from("/etc/madmail/certs/fullchain.pem"),
            key_path: PathBuf::from("/etc/madmail/certs/privkey.pem"),
            acme_email: String::new(),
            generate_certs: true,
            turn_off_tls: true,
            enable_chatmail: true,
            enable_contact_sharing: true,
            enable_ss: true,
            enable_turn: true,
            enable_iroh: false,
            turn_port: "3478".into(),
            turn_secret: String::new(),
            turn_ttl: 86400,
            ss_addr: "0.0.0.0:8388".into(),
            ss_password: String::new(),
            ss_cipher: "aes-128-gcm".into(),
            language: "en".into(),
            config_dir: PathBuf::from("/etc/madmail"),
            config_path: PathBuf::from("/etc/madmail/madmail.conf"),
            paths_explicit: false,
            use_default_systemd_paths: true,
            system_install: true,
            skip_user: false,
            skip_systemd: false,
            generated: String::new(),
        }
    }

    #[test]
    fn default_paths_use_state_and_configuration_directory() {
        let unit = render_systemd_unit(&sample_cfg());
        assert!(unit.contains("StateDirectory=madmail"));
        assert!(unit.contains("ConfigurationDirectory=madmail"));
        assert!(unit.contains(
            "ExecStart=/usr/local/bin/madmail --config /etc/madmail/madmail.conf run --libexec /var/lib/madmail"
        ));
    }

    #[test]
    fn explicit_paths_reflected_without_state_directory() {
        let mut cfg = sample_cfg();
        cfg.paths_explicit = true;
        cfg.use_default_systemd_paths = false;
        cfg.config_dir = PathBuf::from("/tmp/mm");
        cfg.config_path = PathBuf::from("/tmp/mm/madmail.conf");
        cfg.state_dir = PathBuf::from("/tmp/sd");
        cfg.cert_path = PathBuf::from("/tmp/mm/certs/fullchain.pem");
        cfg.key_path = PathBuf::from("/tmp/mm/certs/privkey.pem");

        let unit = render_systemd_unit(&cfg);
        assert!(!unit.contains("StateDirectory="));
        assert!(!unit.contains("ConfigurationDirectory="));
        assert!(unit.contains("WorkingDirectory=/tmp/sd"));
        assert!(unit.contains("ReadWritePaths=/tmp/sd /tmp/mm"));
        assert!(unit.contains(
            "ExecStart=/usr/local/bin/madmail --config /tmp/mm/madmail.conf run --libexec /tmp/sd"
        ));
    }
}
