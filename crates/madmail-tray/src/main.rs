// Copyright (C) 2026 themadorg
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Madmail system tray helper for Windows operators.
//!
//! Cross-platform CLI / smoke mode works everywhere; the interactive tray UI is
//! Windows-only (`tray-icon`).

mod ops;
mod paths;
#[cfg(windows)]
mod tray_win;

use std::path::PathBuf;

use chatmail_config::DEFAULT_WINDOWS_SERVICE_NAME;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "madmail-tray",
    about = "Madmail tray helper — status, admin, service control"
)]
struct Cli {
    /// Configuration file (default: %ProgramData%\\Madmail\\config\\madmail.conf on Windows).
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// State directory (default: %ProgramData%\\Madmail\\data on Windows).
    #[arg(long, alias = "libexec", global = true)]
    state_dir: Option<PathBuf>,

    /// Windows / systemd service name.
    #[arg(long, default_value = DEFAULT_WINDOWS_SERVICE_NAME, global = true)]
    service_name: String,

    /// Query paths + service once, print a report, exit 0 (CI / smoke).
    #[arg(long)]
    smoke_exit: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Print service / path status and exit.
    Status,
    /// Open the admin UI in the default browser.
    OpenAdmin,
    /// Print the admin token if present.
    Token,
    /// Start the Madmail service (`madmail service start`).
    Start,
    /// Stop the Madmail service.
    Stop,
    /// Register HKCU Run autostart for this tray binary (Windows).
    InstallAutostart,
    /// Remove HKCU Run autostart entry (Windows).
    UninstallAutostart,
}

fn main() {
    let cli = Cli::parse();
    let (def_cfg, def_state) = paths::default_layout();
    let config = cli.config.unwrap_or(def_cfg);
    let state_dir = cli.state_dir.unwrap_or(def_state);
    let service_name = cli.service_name;

    if cli.smoke_exit {
        let r = ops::smoke_report(&config, &state_dir, &service_name);
        ops::print_smoke_report(&r);
        // Always exit 0 for CI: missing install is still a valid smoke.
        std::process::exit(0);
    }

    match cli.command {
        Some(Commands::Status) => {
            let r = ops::smoke_report(&config, &state_dir, &service_name);
            ops::print_smoke_report(&r);
        }
        Some(Commands::OpenAdmin) => {
            let url = paths::admin_url(&config);
            if let Err(e) = ops::open_url(&url) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
            println!("Opened {url}");
        }
        Some(Commands::Token) => match paths::read_admin_token(&state_dir) {
            Some(t) => println!("{t}"),
            None => {
                eprintln!(
                    "Error: admin token not found at {}",
                    paths::admin_token_path(&state_dir).display()
                );
                std::process::exit(1);
            }
        },
        Some(Commands::Start) => {
            let madmail = paths::madmail_binary();
            if let Err(e) = ops::run_madmail_service(&madmail, &config, &state_dir, "start") {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::Stop) => {
            let madmail = paths::madmail_binary();
            if let Err(e) = ops::run_madmail_service(&madmail, &config, &state_dir, "stop") {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Some(Commands::InstallAutostart) => {
            if let Err(e) = install_autostart() {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
            println!("✓ Autostart registered (HKCU Run / MadmailTray)");
        }
        Some(Commands::UninstallAutostart) => {
            if let Err(e) = uninstall_autostart() {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
            println!("✓ Autostart removed");
        }
        None => {
            #[cfg(windows)]
            {
                if let Err(e) = tray_win::run(config, state_dir) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(windows))]
            {
                eprintln!(
                    "Interactive tray UI is Windows-only.\n\
                     Use: madmail-tray --smoke-exit | status | open-admin | token\n\
                     Or run on Windows for the system tray."
                );
                std::process::exit(2);
            }
        }
    }
}

fn install_autostart() -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = format!("\"{}\"", exe.display());
        let status = std::process::Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "MadmailTray",
                "/t",
                "REG_SZ",
                "/d",
                &value,
                "/f",
            ])
            .status()
            .map_err(|e| format!("reg: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("reg add failed ({:?})", status.code()))
        }
    }
    #[cfg(not(windows))]
    {
        Err("install-autostart is only supported on Windows".into())
    }
}

fn uninstall_autostart() -> Result<(), String> {
    #[cfg(windows)]
    {
        let status = std::process::Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "MadmailTray",
                "/f",
            ])
            .status()
            .map_err(|e| format!("reg: {e}"))?;
        // Missing value is fine.
        let _ = status;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("uninstall-autostart is only supported on Windows".into())
    }
}
