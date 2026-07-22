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

//! Windows Service Control Manager host (`--service` process entry).

#![cfg(windows)]

use std::ffi::OsString;
use std::sync::mpsc;
use std::time::Duration;

use chatmail_config::{Cli, Command, DEFAULT_WINDOWS_SERVICE_NAME};
use windows_service::define_windows_service;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;

use crate::boot;
use crate::ctl::service_cmd;

const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

define_windows_service!(ffi_service_main, service_main);

/// Block in `StartServiceCtrlDispatcher` until the service stops.
pub fn run_service_dispatcher() -> Result<(), String> {
    service_dispatcher::start(DEFAULT_WINDOWS_SERVICE_NAME, ffi_service_main)
        .map_err(|e| format!("StartServiceCtrlDispatcher: {e}"))
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service() {
        // Last-resort logging: service has no console.
        let _ = std::fs::write(
            std::env::temp_dir().join("madmail-service-error.txt"),
            e.as_bytes(),
        );
    }
}

fn run_service() -> Result<(), String> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle =
        service_control_handler::register(DEFAULT_WINDOWS_SERVICE_NAME, event_handler)
            .map_err(|e| format!("register service control handler: {e}"))?;

    status_handle
        .set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 1,
            wait_hint: Duration::from_secs(60),
            process_id: None,
        })
        .map_err(|e| format!("set StartPending: {e}"))?;

    let args = parse_service_cli_args()?;

    status_handle
        .set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .map_err(|e| format!("set Running: {e}"))?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;

    let shutdown = async move {
        // Bridge blocking SCM channel into async.
        let _ = tokio::task::spawn_blocking(move || {
            let _ = shutdown_rx.recv();
        })
        .await;
    };

    let boot_result = rt.block_on(boot::run_until(args, shutdown));

    status_handle
        .set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: match &boot_result {
                Ok(()) => ServiceExitCode::Win32(0),
                Err(_) => ServiceExitCode::Win32(1),
            },
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .map_err(|e| format!("set Stopped: {e}"))?;

    boot_result.map_err(|e| e.to_string())
}

fn parse_service_cli_args() -> Result<chatmail_config::Args, String> {
    let argv = service_cmd::argv_without_service_flag();
    let cli = Cli::try_parse_from(argv).map_err(|e| format!("parse service CLI: {e}"))?;
    match cli.command {
        None | Some(Command::Run) => Ok(cli.args),
        Some(other) => Err(format!(
            "Windows service binPath must run the server (got {other:?}); expected `run`"
        )),
    }
}

// Re-export clap TryParse for service host without polluting main.
use clap::Parser;
