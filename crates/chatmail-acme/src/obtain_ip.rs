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

//! Let's Encrypt short-lived certificates for public IP addresses (HTTP-01).

use std::net::IpAddr;
use std::path::Path;

use chatmail_types::{ChatmailError, Result};
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt,
    NewAccount, NewOrder, OrderStatus, RetryPolicy,
};
use tracing::info;

use crate::http01::Http01Solver;
use crate::obtain::{cert_needs_renewal, write_pem_pair, ObtainOptions};

/// Let's Encrypt profile for ~6-day IP certificates.
pub const LETS_ENCRYPT_SHORTLIVED_PROFILE: &str = "shortlived";

/// Renew IP certificates when fewer than this many days remain (~160h lifetime).
pub const IP_CERT_RENEW_WITHIN_DAYS: u32 = 4;

/// Parse and validate a public IP suitable for Let's Encrypt IP certificates.
pub fn parse_public_ip(domain: &str) -> Result<IpAddr> {
    let bare = domain.trim().trim_matches(|c| c == '[' || c == ']');
    let ip: IpAddr = bare
        .parse()
        .map_err(|_| ChatmailError::config(format!("not an IP address: {domain:?}")))?;
    if !is_public_ip(&ip) {
        return Err(ChatmailError::config(format!(
            "Let's Encrypt IP certificates require a public IP address, not {ip}"
        )));
    }
    Ok(ip)
}

/// Returns true when `ip` is globally routable (not private, loopback, link-local, etc.).
pub fn is_public_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.octets()[0] == 0)
        }
        IpAddr::V6(v6) => {
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || (v6.segments()[0] & 0xff00) == 0xfe00)
        }
    }
}

pub async fn obtain_ip_certificate(opts: &ObtainOptions) -> Result<()> {
    let ip = parse_public_ip(&opts.domain)?;

    if opts.skip_if_valid
        && !cert_needs_renewal(&opts.cert_path(), IP_CERT_RENEW_WITHIN_DAYS)?
    {
        info!(
            cert = %opts.cert_path().display(),
            "existing IP certificate still valid; skipping issuance"
        );
        println!(
            "Certificate at {} is still valid (≥{IP_CERT_RENEW_WITHIN_DAYS} days left). Use `certificate regenerate` to force renewal.",
            opts.cert_path().display()
        );
        return Ok(());
    }

    ensure_http01_listen_available(&opts.http_listen)?;

    println!(
        "Starting HTTP-01 challenge listener on {} (short-lived IP certificate)…",
        opts.http_listen
    );
    let solver = Http01Solver::new();
    let http_handle = solver
        .start(&opts.http_listen)
        .map_err(|e| ChatmailError::config(format!("bind HTTP-01 on {}: {e}", opts.http_listen)))?;

    let directory_url = if opts.staging {
        LetsEncrypt::Staging.url().to_owned()
    } else {
        LetsEncrypt::Production.url().to_owned()
    };

    let account = load_or_create_le_account(opts, &directory_url).await?;

    let identifiers = [Identifier::Ip(ip)];
    let new_order =
        NewOrder::new(&identifiers).profile(LETS_ENCRYPT_SHORTLIVED_PROFILE);

    println!("Requesting short-lived certificate for {ip} from Let's Encrypt…");
    let mut order = account
        .new_order(&new_order)
        .await
        .map_err(map_instant_acme)?;

    let mut authorizations = order.authorizations();
    while let Some(result) = authorizations.next().await {
        let mut authz = result.map_err(map_instant_acme)?;
        match authz.status {
            AuthorizationStatus::Valid => continue,
            AuthorizationStatus::Pending => {}
            other => {
                return Err(ChatmailError::config(format!(
                    "unexpected authorization status: {other:?}"
                )));
            }
        }

        let mut challenge = authz
            .challenge(ChallengeType::Http01)
            .ok_or_else(|| ChatmailError::config("no http-01 challenge for IP identifier"))?;

        let host = challenge.identifier().to_string();
        let token = challenge.token.clone();
        let key_auth = challenge.key_authorization().as_str().to_string();

        solver.present(host.clone(), token.clone(), key_auth);
        challenge.set_ready().await.map_err(map_instant_acme)?;
        info!(%host, %token, "http-01 challenge marked ready (waiting for Let's Encrypt validation)");
    }

    let retry = RetryPolicy::new().timeout(std::time::Duration::from_secs(60));
    let status = order.poll_ready(&retry).await.map_err(map_instant_acme)?;
    if status != OrderStatus::Ready {
        return Err(order_not_ready_error(status, ip));
    }

    let key_pem = order.finalize().await.map_err(map_instant_acme)?;
    let chain_pem = order
        .poll_certificate(&RetryPolicy::default())
        .await
        .map_err(map_instant_acme)?;

    write_pem_pair(
        &opts.cert_path(),
        &opts.key_path(),
        chain_pem.as_bytes(),
        key_pem.as_bytes(),
    )?;

    http_handle
        .stop()
        .await
        .map_err(ChatmailError::config)?;

    println!(
        "✓ Short-lived IP certificate written:\n  {}\n  {}",
        opts.cert_path().display(),
        opts.key_path().display()
    );
    Ok(())
}

async fn load_or_create_le_account(
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
        .create(
            &new_account,
            directory_url.to_owned(),
            None,
        )
        .await
        .map_err(map_instant_acme)?;

    save_le_account(&cred_path, &credentials)?;
    Ok(account)
}

fn save_le_account(path: &Path, credentials: &AccountCredentials) -> Result<()> {
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

fn map_instant_acme(e: instant_acme::Error) -> ChatmailError {
    ChatmailError::config(e.to_string())
}

/// Port 80 must be free for our temporary HTTP-01 responder (not an already-running madmail/nginx).
fn ensure_http01_listen_available(addr: &std::net::SocketAddr) -> Result<()> {
    std::net::TcpListener::bind(addr).map_err(|e| {
        ChatmailError::config(format!(
            "cannot bind {addr} for HTTP-01 ({e}). Stop any service using port {} first \
             (e.g. systemctl stop madmail, or another web server), then retry.",
            addr.port()
        ))
    })?;
    Ok(())
}

fn order_not_ready_error(status: OrderStatus, ip: IpAddr) -> ChatmailError {
    ChatmailError::config(format!(
        "Let's Encrypt rejected the HTTP-01 challenge for {ip} (order status: {status:?}).\n\
         Common causes:\n\
         • Port 80 is already in use — validators reach another process, not this installer. \
           Run `systemctl stop madmail` (and nginx/apache if any) before `install --auto-ip-cert`.\n\
         • Inbound TCP 80 is blocked by a firewall or cloud security group.\n\
         • This machine is not reachable on the public IP {ip} (install must run on the host that owns the IP).\n\
         • Rate limits — wait an hour or use `madmail certificate get --staging` for testing."
    ))
}

/// Let's Encrypt requires `mailto:` contacts with a DNS domain, not an IP literal.
fn acme_mailto_contact(email: &str) -> Result<String> {
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
    if domain.parse::<std::net::IpAddr>().is_ok() {
        return Err(ChatmailError::config(format!(
            "Let's Encrypt requires --acme-email with a DNS domain (got {email:?}); \
             addresses like admin@{domain} are not accepted"
        )));
    }
    Ok(format!("mailto:{local}@{domain}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn acme_contact_rejects_ip_domain() {
        assert!(acme_mailto_contact("admin@1.2.3.4").is_err());
        assert_eq!(
            acme_mailto_contact("admin@example.com").unwrap(),
            "mailto:admin@example.com"
        );
    }

    #[test]
    fn public_ip_validation() {
        assert!(is_public_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_public_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(!is_public_ip(&IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(parse_public_ip("[1.1.1.1]").is_ok());
        assert!(parse_public_ip("not-an-ip").is_err());
    }
}
