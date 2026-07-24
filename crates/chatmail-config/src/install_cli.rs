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

//! Flags for `madmail install` and `madmail certificate` / `certificate autocert` (Madmail-compatible).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// `madmail install` — server setup (see `context/madmail/docs/chatmail/certificate.md`).
#[derive(Debug, Parser, Clone)]
pub struct InstallArgs {
    /// Non-interactive install (required for scripts).
    #[arg(long, short = 'n')]
    pub non_interactive: bool,

    /// Quick IP-based chatmail setup (`--ip` sets domain/hostname).
    #[arg(long, short = 's')]
    pub simple: bool,

    #[arg(long)]
    pub domain: Option<String>,

    #[arg(long)]
    pub hostname: Option<String>,

    /// Public IP (`--simple` sets domain from this).
    #[arg(long)]
    pub ip: Option<String>,

    /// Config directory (default: `/etc/madmail`).
    #[arg(long)]
    pub config_dir: Option<PathBuf>,

    /// State directory (default: `/var/lib/<binary>` for `--simple` / system install).
    #[arg(long)]
    pub state_dir: Option<PathBuf>,

    /// TLS mode: `autocert`, `file`, `self_signed` (auto-detected if omitted).
    #[arg(long)]
    pub tls_mode: Option<String>,

    #[arg(long)]
    pub cert_path: Option<PathBuf>,

    #[arg(long)]
    pub key_path: Option<PathBuf>,

    #[arg(long)]
    pub acme_email: Option<String>,

    #[arg(long)]
    pub enable_chatmail: bool,

    /// Enable Shadowsocks proxy in generated `chatmail` blocks (Madmail `--enable-ss`).
    #[arg(long)]
    pub enable_ss: bool,

    /// Enable Iroh relay + IMAP discovery (`iroh_relay_url` in `imap` block).
    #[arg(long)]
    pub enable_iroh: bool,

    #[arg(long)]
    pub turn_off_tls: bool,

    #[arg(long)]
    pub dry_run: bool,

    /// Do not install systemd unit files.
    #[arg(long)]
    pub skip_systemd: bool,

    /// Do not create the service system user (`useradd`).
    #[arg(long)]
    pub skip_user: bool,

    /// Register a Windows service after install (no-op notice on Unix).
    #[arg(long)]
    pub install_service: bool,

    /// Start the Windows service after install (implies service registration on Windows).
    #[arg(long)]
    pub start_service: bool,

    /// Open Windows Firewall rules for standard mail/HTTP ports (no-op notice on Unix).
    #[arg(long)]
    pub firewall: bool,

    /// Install path for the binary (default: `/usr/local/bin/<argv0>`).
    #[arg(long)]
    pub binary_path: Option<PathBuf>,

    /// Obtain Let's Encrypt cert during install (`autocert`, or `file` when PEMs are missing; needs port 80).
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub obtain_certificate: bool,

    /// Skip Let's Encrypt issuance during install (use with existing PEMs or `self_signed`).
    #[arg(long = "no-obtain-certificate", action = clap::ArgAction::SetTrue)]
    pub no_obtain_certificate: bool,

    /// Only create TLS directories and obtain a certificate; skip config, DB, and systemd.
    #[arg(long)]
    pub cert_only: bool,

    /// HTTP-01 listener for certificate issuance (port 80 must be free).
    #[arg(long, default_value = "0.0.0.0:80")]
    pub http_listen: String,

    /// Obtain a Let's Encrypt short-lived certificate for `--ip` (HTTP-01 on port 80).
    #[arg(long)]
    pub auto_ip_cert: bool,

    /// Website/UI language: `en`, `fa`, `ru`, `es` (Madmail `--lang`; seeds `__LANGUAGE__` in DB).
    #[arg(long, default_value = "en")]
    pub lang: String,
}

/// `madmail certificate` — Let's Encrypt via instant-acme HTTP-01.
#[derive(Debug, Subcommand, Clone)]
pub enum CertificateCommand {
    /// Obtain certificate if missing or expiring within 30 days.
    Get(CertificateArgs),
    /// Force new certificate issuance.
    Regenerate(CertificateArgs),
    /// Show certificate management mode and validity.
    Status,
    /// Enable or inspect in-process Let's Encrypt auto-renewal.
    #[command(subcommand)]
    Autocert(CertificateAutocertCommand),
}

/// `madmail certificate autocert` — persist `tls_mode = autocert` and renewal email.
#[derive(Debug, Subcommand, Clone)]
pub enum CertificateAutocertCommand {
    /// Turn on autocert mode and store ACME contact email (optional immediate issuance).
    Enable(CertificateAutocertEnableArgs),
    /// Show autocert mode, contact email, and renewal eligibility.
    Status,
}

#[derive(Debug, Parser, Clone)]
pub struct CertificateAutocertEnableArgs {
    /// ACME contact email (Let's Encrypt account).
    #[arg(long)]
    pub email: String,

    /// HTTP-01 listener (port 80 must be free when `--obtain` is used).
    #[arg(long, default_value = "0.0.0.0:80")]
    pub http_listen: String,

    /// Use Let's Encrypt staging (for tests).
    #[arg(long)]
    pub staging: bool,

    /// Obtain certificate immediately after enabling (needs port 80 free).
    #[arg(long, default_value_t = true)]
    pub obtain: bool,
}

#[derive(Debug, Parser, Clone)]
pub struct CertificateArgs {
    /// DNS name (default: `primary_domain` from config).
    #[arg(long)]
    pub domain: Option<String>,

    /// ACME contact email (default: `admin@<domain>`).
    #[arg(long)]
    pub email: Option<String>,

    /// HTTP-01 listener (port 80 must be free).
    #[arg(long, default_value = "0.0.0.0:80")]
    pub http_listen: String,

    /// Use Let's Encrypt staging (for tests).
    #[arg(long)]
    pub staging: bool,

    /// Force issuance on `get` even if current cert is still valid.
    #[arg(long)]
    pub force: bool,
}
