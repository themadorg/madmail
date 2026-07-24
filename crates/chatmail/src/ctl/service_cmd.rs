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

//! `madmail service` — Windows SCM lifecycle (install / start / stop / status).

use std::path::Path;
use std::process::Command;

use chatmail_config::{Args, ServiceCommand};
use chatmail_types::{ChatmailError, Result};

use super::output::CtlOut;

const SERVICE_FLAG: &str = "--service";

pub async fn service(args: &Args, cmd: &ServiceCommand) -> Result<()> {
    match cmd {
        ServiceCommand::Install { name, start } => install(args, name, *start),
        ServiceCommand::Uninstall { name } => uninstall(args, name),
        ServiceCommand::Start { name } => start_service(args, name),
        ServiceCommand::Stop { name } => stop_service(args, name),
        ServiceCommand::Status { name } => status(args, name),
    }
}

fn require_windows() -> Result<()> {
    #[cfg(windows)]
    {
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err(ChatmailError::config(
            "madmail service is only supported on Windows\n\
             On Linux/Unix use systemd (madmail install creates a unit when run as root).",
        ))
    }
}

fn install(args: &Args, name: &str, start_after: bool) -> Result<()> {
    require_windows()?;
    let out = CtlOut::from_args(args, "service");
    // Prefer sibling/Program Files madmail.exe — never register madmail-tray as the service.
    let exe = resolve_service_executable()?;
    let bin_path = service_bin_path(&exe, &args.config, &args.state_dir);
    create_or_update_service(name, &exe, &args.config, &args.state_dir)?;
    if !out.is_json() {
        println!("✓ Registered Windows service {name}");
        println!("  binPath: {bin_path}");
        println!("  config:  {}", args.config.display());
        println!("  state:   {}", args.state_dir.display());
    }
    if start_after {
        start_service(args, name)?;
    }
    if out.is_json() {
        out.emit(serde_json::json!({
            "installed": true,
            "name": name,
            "bin_path": bin_path,
            "config": args.config.display().to_string(),
            "state_dir": args.state_dir.display().to_string(),
            "started": start_after,
        }))?;
    }
    Ok(())
}

/// Binary that the SCM must launch (always `madmail.exe`, never the tray).
fn resolve_service_executable() -> Result<std::path::PathBuf> {
    let current = std::env::current_exe().map_err(|e| {
        ChatmailError::config(format!(
            "resolve current executable for service binPath: {e}"
        ))
    })?;
    let name = current
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name == "madmail.exe" || name == "madmail" {
        return Ok(current);
    }
    if let Some(dir) = current.parent() {
        let sibling = dir.join("madmail.exe");
        if sibling.is_file() {
            return Ok(sibling);
        }
    }
    // Last resort: still use current (may be wrong if tray-only install).
    Ok(current)
}

fn uninstall(args: &Args, name: &str) -> Result<()> {
    require_windows()?;
    let out = CtlOut::from_args(args, "service");
    let _ = stop_service_quiet(name);
    delete_service(name)?;
    if out.is_json() {
        out.emit(serde_json::json!({ "uninstalled": true, "name": name }))?;
    } else {
        println!("✓ Removed Windows service {name}");
    }
    Ok(())
}

fn start_service(args: &Args, name: &str) -> Result<()> {
    require_windows()?;
    let out = CtlOut::from_args(args, "service");
    run_sc(&["start", name], &format!("start service {name}"))?;
    if out.is_json() {
        out.emit(serde_json::json!({ "started": true, "name": name }))?;
    } else {
        println!("✓ Started service {name}");
    }
    Ok(())
}

fn stop_service(args: &Args, name: &str) -> Result<()> {
    require_windows()?;
    let out = CtlOut::from_args(args, "service");
    stop_service_quiet(name)?;
    if out.is_json() {
        out.emit(serde_json::json!({ "stopped": true, "name": name }))?;
    } else {
        println!("✓ Stopped service {name}");
    }
    Ok(())
}

fn stop_service_quiet(name: &str) -> Result<()> {
    // Already stopped is not fatal for uninstall.
    match run_sc(&["stop", name], &format!("stop service {name}")) {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("1062") || msg.to_lowercase().contains("not started") {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

fn status(args: &Args, name: &str) -> Result<()> {
    require_windows()?;
    let out = CtlOut::from_args(args, "service");
    let state = query_service_state(name)?;
    if out.is_json() {
        out.emit(serde_json::json!({ "name": name, "state": state }))?;
    } else {
        println!("Service: {name}");
        println!("State:   {state}");
    }
    Ok(())
}

/// Build SCM `binPath=` value: `"exe" --service --config "…" run --libexec "…"`.
pub fn service_bin_path(exe: &Path, config: &Path, state_dir: &Path) -> String {
    format!(
        "\"{}\" {SERVICE_FLAG} --config \"{}\" run --libexec \"{}\"",
        exe.display(),
        config.display(),
        state_dir.display()
    )
}

/// True when this process was launched as a Windows service (`--service` in argv).
pub fn argv_has_service_flag() -> bool {
    std::env::args().any(|a| a == SERVICE_FLAG)
}

/// argv for clap with `--service` removed (service host re-parses after SCM connect).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn argv_without_service_flag() -> Vec<std::ffi::OsString> {
    std::env::args_os().filter(|a| a != SERVICE_FLAG).collect()
}

/// Register or update the Madmail SCM service via the Win32 service APIs.
///
/// Prefer this over shelling out to `sc.exe`: Rust's default `Command` quoting
/// turns `start= auto` into `"start= auto"`, which `sc` rejects with exit 1639.
#[cfg(windows)]
fn create_or_update_service(name: &str, exe: &Path, config: &Path, state_dir: &Path) -> Result<()> {
    use std::ffi::OsString;
    use std::time::Duration;

    use windows_service::service::{
        ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl,
        ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType,
        ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
    let manager = ServiceManager::local_computer(None::<&str>, manager_access)
        .map_err(|e| ChatmailError::config(format!("OpenSCManager (need elevated admin): {e}")))?;

    let service_info = ServiceInfo {
        name: OsString::from(name),
        display_name: OsString::from("Madmail"),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe.to_path_buf(),
        launch_arguments: vec![
            OsString::from(SERVICE_FLAG),
            OsString::from("--config"),
            config.as_os_str().to_os_string(),
            OsString::from("run"),
            OsString::from("--libexec"),
            state_dir.as_os_str().to_os_string(),
        ],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };

    let service_access = ServiceAccess::QUERY_CONFIG
        | ServiceAccess::CHANGE_CONFIG
        | ServiceAccess::START
        | ServiceAccess::DELETE;

    // 1073 = ERROR_SERVICE_EXISTS
    let service = match manager.create_service(&service_info, service_access) {
        Ok(svc) => svc,
        Err(windows_service::Error::Winapi(io)) if io.raw_os_error() == Some(1073) => {
            let svc = manager
                .open_service(name, service_access)
                .map_err(|e| ChatmailError::config(format!("open existing service {name}: {e}")))?;
            svc.change_config(&service_info)
                .map_err(|e| ChatmailError::config(format!("update service {name} config: {e}")))?;
            svc
        }
        Err(e) => {
            return Err(ChatmailError::config(format!("create service {name}: {e}")));
        }
    };

    // Best-effort metadata; registration already succeeded.
    let _ = service.set_description("Madmail chatmail / mail server");
    let failure_actions = ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_millis(5_000),
            },
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_millis(5_000),
            },
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_millis(5_000),
            },
        ]),
    };
    let _ = service.update_failure_actions(failure_actions);

    Ok(())
}

#[cfg(not(windows))]
fn create_or_update_service(
    _name: &str,
    _exe: &Path,
    _config: &Path,
    _state_dir: &Path,
) -> Result<()> {
    require_windows()
}

#[cfg(windows)]
fn delete_service(name: &str) -> Result<()> {
    run_sc(&["delete", name], &format!("delete service {name}"))
}

#[cfg(not(windows))]
fn delete_service(_name: &str) -> Result<()> {
    require_windows()
}

#[cfg(windows)]
fn query_service_state(name: &str) -> Result<String> {
    let output = sc_command(&["query", name])
        .output()
        .map_err(|e| ChatmailError::config(format!("sc query {name}: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    if !output.status.success() {
        // 1060 = service does not exist — a normal status, not a hard failure.
        if combined.contains("1060") || combined.to_ascii_lowercase().contains("does not exist") {
            return Ok("NotInstalled".into());
        }
        return Err(ChatmailError::config(format!(
            "sc query {name} failed: {}",
            combined.trim()
        )));
    }
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("STATE") {
            // e.g. STATE              : 4  RUNNING
            let state = rest
                .split_whitespace()
                .last()
                .unwrap_or("UNKNOWN")
                .to_string();
            return Ok(state);
        }
    }
    Ok("UNKNOWN".into())
}

#[cfg(not(windows))]
fn query_service_state(_name: &str) -> Result<String> {
    require_windows().map(|_| unreachable!())
}

fn run_sc(args: &[&str], action: &str) -> Result<()> {
    let output = sc_command(args)
        .output()
        .map_err(|e| ChatmailError::config(format!("sc {action}: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(ChatmailError::config(format!(
        "sc {action} failed (exit {:?}): {}{}",
        output.status.code(),
        stdout.trim(),
        stderr.trim()
    )))
}

/// Build an `sc.exe` command (used for start/stop/delete/query).
///
/// Service **create/config** uses the Win32 API ([`create_or_update_service`]) so
/// we never depend on `sc` option quoting. Remaining `sc` calls still use
/// [`CommandExt::raw_arg`] on Windows: if a future caller passes `key= value`
/// tokens, Rust's default quoting must not wrap them as `"start= auto"` (exit 1639).
fn sc_command(args: &[&str]) -> Command {
    let mut cmd = Command::new("sc");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        for a in args {
            cmd.raw_arg(a);
        }
    }
    #[cfg(not(windows))]
    {
        cmd.args(args);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn service_bin_path_includes_service_flag_and_paths() {
        let p = service_bin_path(
            Path::new(r"C:\Program Files\Madmail\madmail.exe"),
            Path::new(r"C:\ProgramData\Madmail\config\madmail.conf"),
            Path::new(r"C:\ProgramData\Madmail\data"),
        );
        assert!(p.contains(SERVICE_FLAG), "{p}");
        assert!(p.contains("run"), "{p}");
        assert!(p.contains("--libexec"), "{p}");
        assert!(p.contains("madmail.exe"), "{p}");
    }

    #[test]
    fn service_bin_path_is_suitable_for_scm_image_path() {
        let bin = service_bin_path(
            Path::new(r"C:\Program Files\Madmail\madmail.exe"),
            Path::new(r"C:\ProgramData\Madmail\config\madmail.conf"),
            Path::new(r"C:\ProgramData\Madmail\data"),
        );
        // CreateService uses exe + argv; ImagePath-style string still used for display/logging.
        assert!(bin.starts_with('"'), "{bin}");
        assert!(bin.contains("--service"), "{bin}");
        assert!(bin.contains("--config"), "{bin}");
        assert!(bin.contains("run"), "{bin}");
        assert!(bin.contains("--libexec"), "{bin}");
        // sc start/stop/delete still go through sc_command (raw_arg on Windows).
        let _ = sc_command(&["query", "Madmail"]);
    }

    #[test]
    fn argv_without_service_flag_filters() {
        use std::ffi::OsString;
        // Pure unit: construct filter logic equivalent
        let raw = [
            OsString::from("madmail"),
            OsString::from("--service"),
            OsString::from("--config"),
            OsString::from("c.conf"),
            OsString::from("run"),
        ];
        let filtered: Vec<_> = raw.into_iter().filter(|a| a != SERVICE_FLAG).collect();
        assert_eq!(filtered.len(), 4);
        assert!(!filtered.iter().any(|a| a == SERVICE_FLAG));
    }

    #[tokio::test]
    async fn service_commands_error_on_non_windows() {
        #[cfg(not(windows))]
        {
            let args = Args {
                config: PathBuf::from("/tmp/x.conf"),
                state_dir: PathBuf::from("/tmp/x"),
                boot_once: false,
                json: false,
            };
            let err = service(
                &args,
                &ServiceCommand::Status {
                    name: "Madmail".into(),
                },
            )
            .await
            .unwrap_err();
            assert!(
                err.to_string().contains("only supported on Windows"),
                "{err}"
            );
        }
    }
}
