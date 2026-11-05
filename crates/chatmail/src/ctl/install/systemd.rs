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

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use chatmail_types::{ChatmailError, Result};

use super::config::InstallConfig;

pub fn install_unit(cfg: &InstallConfig) -> Result<()> {
    let unit_path = Path::new("/etc/systemd/system").join(format!("{}.service", cfg.binary_name));
    write_unit_file(&unit_path, &render_systemd_unit(cfg), &cfg.binary_name)?;
    println!("✓ Wrote {}", unit_path.display());
    Ok(())
}

/// Replace the unit atomically. A symlink (a `systemctl mask` link to /dev/null or
/// a link to another file) is refused instead of being written through.
fn write_unit_file(unit_path: &Path, body: &str, name: &str) -> Result<()> {
    let write_err =
        |e: std::io::Error| ChatmailError::config(format!("write {}: {e}", unit_path.display()));
    match fs::symlink_metadata(unit_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(ChatmailError::config(format!(
                "{} is a symlink (masked?); run `systemctl unmask {name}` and retry",
                unit_path.display()
            )));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(write_err(e)),
    }
    let mut staging = unit_path.as_os_str().to_owned();
    staging.push(".tmp");
    let staging = PathBuf::from(staging);
    // remove_file does not follow a link left at the staging path.
    match fs::remove_file(&staging) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(write_err(e)),
        _ => {}
    }
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&staging)
        .and_then(|mut file| file.write_all(body.as_bytes()))
        .and_then(|()| fs::rename(&staging, unit_path));
    if let Err(e) = written {
        let _ = fs::remove_file(&staging);
        return Err(write_err(e));
    }
    Ok(())
}

// Escape unit specifiers everywhere and command environment expansion in ExecStart.
// For word-split settings (`ExecStart=`, `ReadWritePaths=`) only.
fn unit_path(path: &Path, command: bool) -> String {
    let mut value = path.to_string_lossy().replace('%', "%%");
    if command {
        value = value.replace('$', "$$");
    }
    if value
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\'))
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value
    }
}

// `WorkingDirectory=` takes the whole value verbatim: systemd expands specifiers
// but does not unquote, so only `%` is escaped and no quotes are added.
fn unit_working_directory(path: &Path) -> String {
    path.to_string_lossy().replace('%', "%%")
}

/// Build the `.service` unit body. Default FHS paths use `StateDirectory` /
/// `ConfigurationDirectory`; explicit `--config-dir` / `--state-dir` are reflected
/// in `ExecStart`, `WorkingDirectory`, and `ReadWritePaths` (`WorkingDirectory`
/// is written verbatim, the others are quoted as needed). Both layouts retain
/// `RuntimeDirectory`. Home protection is disabled when a required path needs it.
pub fn render_systemd_unit(cfg: &InstallConfig) -> String {
    let managed_dirs = if cfg.use_default_systemd_paths {
        format!(
            r#"
StateDirectory={name}
ConfigurationDirectory={name}
LogsDirectory={name}"#,
            name = cfg.binary_name,
        )
    } else {
        String::new()
    };

    let protect_home = ![
        &cfg.state_dir,
        &cfg.config_dir,
        &cfg.config_path,
        &cfg.binary_path,
        &cfg.cert_path,
        &cfg.key_path,
    ]
    .iter()
    .any(|path| {
        ["/home", "/root", "/run/user"]
            .iter()
            .any(|prefix| path.starts_with(prefix))
    });

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
WorkingDirectory={working_dir}
RuntimeDirectory={name}
{managed_dirs}
ReadWritePaths={state_dir} {config_dir}

PrivateTmp=true
ProtectHome={protect_home}
ProtectSystem=full
NoNewPrivileges=true

AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE

UMask=0007
LimitNOFILE=131072
LimitNPROC=512

ExecStart={binary} --config {config} run --libexec {exec_state_dir}
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
        working_dir = unit_working_directory(&cfg.state_dir),
        state_dir = unit_path(&cfg.state_dir, false),
        exec_state_dir = unit_path(&cfg.state_dir, true),
        managed_dirs = managed_dirs,
        config_dir = unit_path(&cfg.config_dir, false),
        binary = unit_path(&cfg.binary_path, true),
        config = unit_path(&cfg.config_path, true),
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
    fn home_protection_matches_required_paths() {
        assert!(render_systemd_unit(&sample_cfg()).contains("ProtectHome=true"));
        for prefix in ["/home", "/root", "/run/user"] {
            for field in 0..6 {
                let mut cfg = sample_cfg();
                let path = PathBuf::from(format!("{prefix}/custom/mail"));
                match field {
                    0 => cfg.state_dir = path,
                    1 => cfg.config_dir = path,
                    2 => cfg.config_path = path,
                    3 => cfg.binary_path = path,
                    4 => cfg.cert_path = path,
                    _ => cfg.key_path = path,
                }
                assert!(render_systemd_unit(&cfg).contains("ProtectHome=false"));
            }
        }
        let mut cfg = sample_cfg();
        cfg.state_dir = PathBuf::from("/home-other/mail");
        assert!(render_systemd_unit(&cfg).contains("ProtectHome=true"));
    }

    #[test]
    fn paths_are_escaped_for_systemd_not_interpreted() {
        assert_eq!(
            unit_path(Path::new("/srv/mail data"), false),
            "\"/srv/mail data\""
        );
        assert_eq!(
            unit_path(Path::new("/srv/%n/$HOME"), false),
            "/srv/%%n/$HOME"
        );
        assert_eq!(
            unit_path(Path::new("/srv/%n/$HOME"), true),
            "/srv/%%n/$$HOME"
        );
        assert_eq!(
            unit_path(Path::new("/srv/\"mail"), true),
            "\"/srv/\\\"mail\""
        );
        assert_eq!(
            unit_path(Path::new("/srv/o'brien"), false),
            "\"/srv/o'brien\""
        );
    }

    #[test]
    fn working_directory_is_verbatim_not_quoted() {
        let mut cfg = sample_cfg();
        cfg.use_default_systemd_paths = false;
        cfg.state_dir = PathBuf::from("/srv/mail data");
        let unit = render_systemd_unit(&cfg);
        assert!(unit.contains("\nWorkingDirectory=/srv/mail data\n"));
        assert!(unit.contains("\nReadWritePaths=\"/srv/mail data\" /etc/madmail\n"));
        assert!(unit.contains("run --libexec \"/srv/mail data\"\n"));

        cfg.state_dir = PathBuf::from("/srv/%n/$HOME");
        let unit = render_systemd_unit(&cfg);
        assert!(unit.contains("\nWorkingDirectory=/srv/%%n/$HOME\n"));
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
        assert!(unit.contains("RuntimeDirectory=madmail"));
        assert!(!unit.contains("ConfigurationDirectory="));
        assert!(unit.contains("WorkingDirectory=/tmp/sd"));
        assert!(unit.contains("ReadWritePaths=/tmp/sd /tmp/mm"));
        assert!(unit.contains(
            "ExecStart=/usr/local/bin/madmail --config /tmp/mm/madmail.conf run --libexec /tmp/sd"
        ));
    }

    #[test]
    fn unit_write_refuses_symlinks_and_replaces_regular_files() {
        let dir = tempfile::tempdir().unwrap();
        let unit = dir.path().join("madmail.service");

        write_unit_file(&unit, "first", "madmail").unwrap();
        write_unit_file(&unit, "second", "madmail").unwrap();
        assert_eq!(std::fs::read_to_string(&unit).unwrap(), "second");
        assert!(!dir.path().join("madmail.service.tmp").exists());

        let other = dir.path().join("other.service");
        std::fs::write(&other, "keep").unwrap();
        std::fs::remove_file(&unit).unwrap();
        std::os::unix::fs::symlink(&other, &unit).unwrap();
        let err = write_unit_file(&unit, "body", "madmail")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("symlink") && err.contains("systemctl unmask madmail"),
            "{err}"
        );
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "keep");
        assert!(std::fs::symlink_metadata(&unit)
            .unwrap()
            .file_type()
            .is_symlink());
    }
}
