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

//! `madmail certificate get|regenerate` — Let's Encrypt via lers.

use chatmail_acme::{obtain_certificate, parse_http_listen, ObtainOptions};
use chatmail_config::install_cli::{CertificateArgs, CertificateCommand};
use chatmail_config::{load_config, Args};
use chatmail_types::{wrap_ip_domain, ChatmailError, Result};

pub async fn certificate(args: &Args, cmd: &CertificateCommand) -> Result<()> {
    let cfg = load_config(&args.config)?;
    let domain = cmd_args(cmd)
        .domain
        .clone()
        .or(cfg.primary_domain.clone())
        .or(cfg.mail_domain.clone())
        .or(cfg.hostname.clone())
        .ok_or_else(|| {
            ChatmailError::config("no domain: pass --domain or set primary_domain in config")
        })?;
    let domain = wrap_ip_domain(&domain);
    let bare = domain.trim_matches(|c| c == '[' || c == ']');
    let email = cmd_args(cmd)
        .email
        .clone()
        .unwrap_or_else(|| format!("admin@{bare}"));

    let (skip_if_valid, force_label) = match cmd {
        CertificateCommand::Get { .. } => (!cmd_args(cmd).force, "get"),
        CertificateCommand::Regenerate { .. } => (false, "regenerate"),
    };

    let http_listen = parse_http_listen(&cmd_args(cmd).http_listen)?;
    let opts = ObtainOptions {
        domain: domain.clone(),
        email,
        state_dir: args.state_dir.clone(),
        cert_path: cfg.tls_cert_path.clone(),
        key_path: cfg.tls_key_path.clone(),
        http_listen,
        staging: cmd_args(cmd).staging,
        skip_if_valid,
    };

    println!("madmail certificate {force_label} for {domain}");
    obtain_certificate(&opts).await
}

fn cmd_args(cmd: &CertificateCommand) -> &CertificateArgs {
    match cmd {
        CertificateCommand::Get(a) | CertificateCommand::Regenerate(a) => a,
    }
}
