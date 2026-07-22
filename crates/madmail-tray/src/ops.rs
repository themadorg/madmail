// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Service status and process helpers for the tray.

use std::path::Path;
use std::process::Command;

use crate::paths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceState {
    Running,
    Stopped,
    Unknown(String),
    Unavailable,
}

impl ServiceState {
    pub fn as_str(&self) -> &str {
        match self {
            ServiceState::Running => "Running",
            ServiceState::Stopped => "Stopped",
            ServiceState::Unknown(s) => s.as_str(),
            ServiceState::Unavailable => "Unavailable",
        }
    }
}

/// Query Windows SCM or systemd for Madmail service state.
pub fn query_service_state(service_name: &str) -> ServiceState {
    #[cfg(windows)]
    {
        let output = Command::new("sc").args(["query", service_name]).output();
        match output {
            Ok(o) if o.status.success() => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                for line in stdout.lines() {
                    let line = line.trim();
                    if line.starts_with("STATE") {
                        let state = line
                            .split_whitespace()
                            .last()
                            .unwrap_or("UNKNOWN")
                            .to_string();
                        return match state.to_ascii_uppercase().as_str() {
                            "RUNNING" => ServiceState::Running,
                            "STOPPED" => ServiceState::Stopped,
                            other => ServiceState::Unknown(other.to_string()),
                        };
                    }
                }
                ServiceState::Unknown("UNKNOWN".into())
            }
            Ok(_) => ServiceState::Stopped,
            Err(_) => ServiceState::Unavailable,
        }
    }
    #[cfg(not(windows))]
    {
        let output = Command::new("systemctl")
            .args(["is-active", service_name])
            .output();
        match output {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                match s.as_str() {
                    "active" => ServiceState::Running,
                    "inactive" | "failed" => ServiceState::Stopped,
                    "" => ServiceState::Unavailable,
                    other => ServiceState::Unknown(other.to_string()),
                }
            }
            Err(_) => ServiceState::Unavailable,
        }
    }
}

pub fn run_madmail_service(
    madmail: &Path,
    config: &Path,
    state_dir: &Path,
    action: &str,
) -> Result<(), String> {
    let status = Command::new(madmail)
        .args([
            "--config",
            &config.display().to_string(),
            "--state-dir",
            &state_dir.display().to_string(),
            "service",
            action,
        ])
        .status()
        .map_err(|e| format!("spawn madmail: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("madmail service {action} exit {:?}", status.code()))
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| format!("explorer: {e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("xdg-open: {e}"))?;
        Ok(())
    }
}

pub fn open_url(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        open::that(url).map_err(|e| format!("open url: {e}"))
    }
    #[cfg(not(windows))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("xdg-open: {e}"))?;
        Ok(())
    }
}

/// Snapshot used by `--smoke-exit` and tray status label.
#[derive(Debug)]
pub struct SmokeReport {
    pub config: String,
    pub state_dir: String,
    pub config_exists: bool,
    pub state_exists: bool,
    pub token_present: bool,
    pub service: ServiceState,
    pub admin_url: String,
    pub madmail: String,
}

pub fn smoke_report(config: &Path, state_dir: &Path, service_name: &str) -> SmokeReport {
    SmokeReport {
        config: config.display().to_string(),
        state_dir: state_dir.display().to_string(),
        config_exists: config.is_file(),
        state_exists: state_dir.is_dir(),
        token_present: paths::read_admin_token(state_dir).is_some(),
        service: query_service_state(service_name),
        admin_url: paths::admin_url(config),
        madmail: paths::madmail_binary().display().to_string(),
    }
}

pub fn print_smoke_report(r: &SmokeReport) {
    println!("madmail-tray smoke");
    println!("  config:     {} (exists={})", r.config, r.config_exists);
    println!("  state_dir:  {} (exists={})", r.state_dir, r.state_exists);
    println!("  admin_token present: {}", r.token_present);
    println!("  service:    {}", r.service.as_str());
    println!("  admin_url:  {}", r.admin_url);
    println!("  madmail:    {}", r.madmail);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_report_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("nope.conf");
        let r = smoke_report(&cfg, dir.path(), "Madmail");
        assert!(!r.config_exists);
        assert!(r.state_exists);
        assert!(!r.token_present);
        assert!(r.admin_url.contains("127.0.0.1"));
    }
}
