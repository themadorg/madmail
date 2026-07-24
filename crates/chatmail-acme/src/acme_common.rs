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

//! Shared helpers for instant-acme HTTP-01 issuance (DNS and IP).

use std::net::{IpAddr, SocketAddr};
use std::path::Path;

use chatmail_types::{ChatmailError, Result};
use instant_acme::{Account, AccountCredentials, NewAccount, OrderStatus};

use crate::obtain::ObtainOptions;
use crate::obtain_ip::is_public_ip;

pub fn map_instant_acme(e: instant_acme::Error) -> ChatmailError {
    ChatmailError::config(e.to_string())
}

/// Port 80 must be free for our temporary HTTP-01 responder (not an already-running madmail/nginx).
pub fn ensure_http01_listen_available(addr: &SocketAddr) -> Result<()> {
    std::net::TcpListener::bind(addr).map_err(|e| {
        ChatmailError::config(format!(
            "cannot bind {addr} for HTTP-01 ({e}). Stop any service using port {} first, then retry.\n\
             {}",
            addr.port(),
            http01_stop_hint()
        ))
    })?;
    Ok(())
}

/// Platform-specific hint for freeing port 80 / madmail before HTTP-01.
fn http01_stop_hint() -> &'static str {
    #[cfg(windows)]
    {
        "Windows: `netstat -ano | findstr :80`, stop the listed PID (or `sc.exe stop Madmail` if Madmail is already installed); also check IIS/other web servers."
    }
    #[cfg(not(windows))]
    {
        "Linux/Unix: `systemctl stop madmail` (if installed), and stop nginx/apache or anything else on port 80."
    }
}

pub async fn load_or_create_le_account(
    opts: &ObtainOptions,
    directory_url: &str,
) -> Result<Account> {
    let cred_path = opts.le_account_path();
    if cred_path.is_file() {
        let text = std::fs::read_to_string(&cred_path)
            .map_err(|e| ChatmailError::config(format!("read {}: {e}", cred_path.display())))?;
        let credentials: AccountCredentials = serde_json::from_str(&text)
            .map_err(|e| ChatmailError::config(format!("parse {}: {e}", cred_path.display())))?;
        return Account::builder()
            .map_err(map_instant_acme)?
            .from_credentials(credentials)
            .await
            .map_err(map_instant_acme);
    }

    let contact = acme_mailto_contact(&opts.email)?;
    let contacts = [contact.as_str()];
    let new_account = NewAccount {
        contact: &contacts,
        terms_of_service_agreed: true,
        only_return_existing: false,
    };

    let (account, credentials) = Account::builder()
        .map_err(map_instant_acme)?
        .create(&new_account, directory_url.to_owned(), None)
        .await
        .map_err(map_instant_acme)?;

    save_le_account(&cred_path, &credentials)?;
    Ok(account)
}

pub fn save_le_account(path: &Path, credentials: &AccountCredentials) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ChatmailError::config(format!("create {}: {e}", parent.display())))?;
    }
    let json = serde_json::to_string_pretty(credentials)
        .map_err(|e| ChatmailError::config(e.to_string()))?;
    std::fs::write(path, json)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).ok();
    }
    Ok(())
}

/// Let's Encrypt requires `mailto:` contacts with a DNS domain, not an IP literal.
pub fn acme_mailto_contact(email: &str) -> Result<String> {
    let bare = email.trim().trim_start_matches("mailto:");
    let Some((local, domain)) = bare.split_once('@') else {
        return Err(ChatmailError::config(format!(
            "invalid ACME contact email {email:?} (expected user@domain)"
        )));
    };
    if local.is_empty() || domain.is_empty() {
        return Err(ChatmailError::config(format!(
            "invalid ACME contact email {email:?}"
        )));
    }
    if domain.parse::<IpAddr>().is_ok() {
        return Err(ChatmailError::config(format!(
            "Let's Encrypt requires --acme-email with a DNS domain (got {email:?}); \
             addresses like admin@{domain} are not accepted"
        )));
    }
    Ok(format!("mailto:{local}@{domain}"))
}

pub fn dns_order_not_ready_error(status: OrderStatus, domain: &str) -> ChatmailError {
    ChatmailError::config(format!(
        "Let's Encrypt rejected the HTTP-01 challenge for {domain} (order status: {status:?}).\n\
         Common causes:\n\
         • Port 80 is already in use — validators reach another process, not this installer. {}\n\
         • Inbound TCP 80 is blocked by a firewall or cloud security group.\n\
         • DNS for {domain} does not point to this machine (A/AAAA must match this host's public IP).\n\
         • A CAA DNS record blocks issuance for this hostname — remove or allow `letsencrypt.org`.\n\
         • Rate limits — wait an hour or use `madmail certificate get --staging` for testing.",
        http01_stop_hint()
    ))
}

pub fn ip_order_not_ready_error(status: OrderStatus, ip: IpAddr) -> ChatmailError {
    ChatmailError::config(format!(
        "Let's Encrypt rejected the HTTP-01 challenge for {ip} (order status: {status:?}).\n\
         Common causes:\n\
         • Port 80 is already in use — validators reach another process, not this installer. {}\n\
         • Inbound TCP 80 is blocked by a host or cloud firewall (e.g. Windows Defender Firewall and/or your provider's panel). \
           Local `netstat` free only means nothing is listening — LE still cannot connect if the firewall drops packets. \
           Open inbound TCP 80, then from another network: `curl -v --max-time 10 http://{ip}/` while a test listener is up.\n\
         • This machine is not reachable on the public IP {ip} (install must run on the host that owns the IP).\n\
         • Rate limits — wait an hour or use `madmail certificate get --staging` for testing.",
        http01_stop_hint()
    ))
}

/// Resolve a DNS hostname to a public IP for install-time `$(public_ip)`.
pub fn resolve_domain_to_public_ip(domain: &str) -> Result<String> {
    use std::net::ToSocketAddrs;

    let bare = domain.trim().trim_matches(|c| c == '[' || c == ']');
    let addrs: Vec<_> = format!("{bare}:0")
        .to_socket_addrs()
        .map_err(|e| {
            ChatmailError::config(format!(
                "DNS lookup for {bare} failed: {e}. Ensure an A/AAAA record exists, or pass --ip explicitly."
            ))
        })?
        .map(|a| a.ip())
        .filter(is_public_ip)
        .map(|ip| ip.to_string())
        .collect();

    addrs.into_iter().next().ok_or_else(|| {
        ChatmailError::config(format!(
            "no public IP found in DNS for {bare}. Point the domain's A/AAAA record at this server, or pass --ip."
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acme_contact_rejects_ip_domain() {
        assert!(acme_mailto_contact("admin@1.2.3.4").is_err());
        assert_eq!(
            acme_mailto_contact("admin@example.com").unwrap(),
            "mailto:admin@example.com"
        );
    }
}
