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

//! `madmail install` — Madmail `ctl/install.go` parity for `--simple --ip`.

mod config;
#[cfg(unix)]
mod system;
mod systemd;

use chatmail_acme::{
    generate_self_signed, is_valid_dns_domain, is_valid_ip_for_acme, obtain_certificate,
    ObtainOptions,
};
use chatmail_config::install_cli::InstallArgs;
use chatmail_config::{effective_database_config, is_local_dev_state_dir, AppConfig, Args};
use chatmail_db::{init_db_from_config, set_setting, settings_keys};
use chatmail_types::{wrap_ip_domain, ChatmailError, Result};

use super::language::validate_language_code;
use self::config::{local_domains_for_ip, render_maddy_conf, InstallConfig};

pub async fn install(global: &Args, args: &InstallArgs) -> Result<()> {
    if !args.non_interactive && !args.simple {
        return Err(ChatmailError::config(
            "interactive install is not implemented yet; use --non-interactive or --simple",
        ));
    }

    let mut cfg = InstallConfig::from_args(global, args)?;
    resolve_tls_mode(&mut cfg, args)?;

    println!("Installing {} (TLS mode: {})", cfg.binary_name, cfg.tls_mode);
    println!("  Primary domain: {}", cfg.primary_domain);
    println!("  Hostname:       {}", cfg.hostname);
    println!("  Public IP:      {}", cfg.public_ip);
    println!("  State dir:      {}", cfg.state_dir.display());
    println!("  Config:         {}", cfg.config_path.display());
    println!("  Language:       {}", cfg.language);

    #[cfg(unix)]
    system::require_root_for_system_install(&cfg, args.dry_run)?;
    #[cfg(not(unix))]
    if cfg.system_install {
        return Err(ChatmailError::config(
            "system install is only supported on Unix",
        ));
    }

    if args.dry_run {
        println!("[dry-run] would run full install steps");
        return Ok(());
    }

    #[cfg(unix)]
    {
        system::create_service_user(&cfg, false)?;
    }
    create_directories(&cfg)?;
    setup_certificates(&cfg, args).await?;
    ensure_secrets(&mut cfg)?;
    write_config(&cfg)?;
    seed_install_language(&cfg).await?;
    #[cfg(unix)]
    {
        system::setup_config_permissions(&cfg, false)?;
        system::setup_permissions(&cfg, false)?;
        system::install_binary(&cfg, false)?;
    }
    if !args.skip_systemd {
        systemd::install_unit(&cfg)?;
        systemd::daemon_reload()?;
    }

    print_next_steps(&cfg);
    println!("\nInstallation completed successfully.");
    Ok(())
}

fn resolve_tls_mode(cfg: &mut InstallConfig, args: &InstallArgs) -> Result<()> {
    if args.auto_ip_cert {
        if !is_valid_ip_for_acme(&cfg.public_ip) {
            eprintln!(
                "warning: --auto-ip-cert ignored: {} is not a public IP",
                cfg.public_ip
            );
        } else {
            cfg.tls_mode = "autocert".into();
            cfg.generate_certs = false;
            cfg.turn_off_tls = args.turn_off_tls;
            if cfg.acme_email.is_empty() {
                return Err(ChatmailError::config(
                    "--acme-email is required with --auto-ip-cert (must be user@domain, not user@IP)",
                ));
            }
            return Ok(());
        }
    }

    if !cfg.tls_mode.is_empty() {
        match cfg.tls_mode.as_str() {
            "autocert" => {
                cfg.generate_certs = false;
                if cfg.acme_email.is_empty() {
                    let bare = cfg.primary_domain.trim_matches(|c| c == '[' || c == ']');
                    cfg.acme_email = format!("admin@{bare}");
                }
            }
            "file" => cfg.generate_certs = false,
            "self_signed" => cfg.generate_certs = true,
            other => {
                eprintln!("warning: unknown tls-mode {other:?}, treating as file");
                cfg.tls_mode = "file".into();
                cfg.generate_certs = false;
            }
        }
        return Ok(());
    }

    if cfg.cert_path.is_file() && cfg.key_path.is_file() {
        cfg.tls_mode = "file".into();
        cfg.generate_certs = false;
        return Ok(());
    }

    if is_valid_dns_domain(&cfg.primary_domain) {
        cfg.tls_mode = "autocert".into();
        cfg.generate_certs = false;
        let bare = cfg.primary_domain.trim_matches(|c| c == '[' || c == ']');
        if cfg.acme_email.is_empty() {
            cfg.acme_email = format!("admin@{bare}");
        }
        return Ok(());
    }

    cfg.tls_mode = "self_signed".into();
    cfg.generate_certs = true;
    Ok(())
}

async fn setup_certificates(cfg: &InstallConfig, args: &InstallArgs) -> Result<()> {
    if cfg.generate_certs {
        println!("Generating self-signed certificate…");
        generate_self_signed(
            &cfg.primary_domain,
            &cfg.hostname,
            &cfg.public_ip,
            &cfg.cert_path,
            &cfg.key_path,
        )?;
        return Ok(());
    }

    if cfg.tls_mode == "autocert" && args.obtain_certificate {
        let label = if is_valid_ip_for_acme(&cfg.public_ip) {
            "short-lived IP"
        } else {
            "DNS"
        };
        println!("Obtaining Let's Encrypt {label} certificate (HTTP-01 on port 80)…");
        let opts = ObtainOptions {
            domain: cfg.primary_domain.clone(),
            email: cfg.acme_email.clone(),
            state_dir: cfg.state_dir.clone(),
            cert_path: Some(cfg.cert_path.clone()),
            key_path: Some(cfg.key_path.clone()),
            http_listen: "0.0.0.0:80"
                .parse()
                .expect("valid default listen"),
            staging: false,
            skip_if_valid: false,
        };
        obtain_certificate(&opts).await?;
    } else if cfg.tls_mode == "file" {
        if !cfg.cert_path.is_file() || !cfg.key_path.is_file() {
            return Err(ChatmailError::config(format!(
                "tls-mode file requires existing cert and key:\n  {}\n  {}",
                cfg.cert_path.display(),
                cfg.key_path.display()
            )));
        }
        println!("Using existing certificate files");
    }
    Ok(())
}

fn create_directories(cfg: &InstallConfig) -> Result<()> {
    let config_dir = cfg.config_path.parent().unwrap_or(std::path::Path::new("/etc/madmail"));
    for dir in [
        &cfg.state_dir,
        cfg.state_dir.join("messages").as_path(),
        cfg.state_dir.join("remote_queue").as_path(),
        cfg.state_dir.join("autocert").as_path(),
        config_dir,
        cfg.cert_path.parent().unwrap_or(config_dir),
    ] {
        std::fs::create_dir_all(dir)
            .map_err(|e| ChatmailError::config(format!("mkdir {}: {e}", dir.display())))?;
    }
    Ok(())
}

async fn seed_install_language(cfg: &InstallConfig) -> Result<()> {
    let mut app_config = AppConfig::default();
    app_config.state_dir = Some(cfg.state_dir.clone());
    app_config.language = Some(cfg.language.clone());

    let database = effective_database_config(&cfg.state_dir, &app_config);
    let pool = init_db_from_config(&database).await?;
    set_setting(&pool, settings_keys::LANGUAGE, &cfg.language).await?;
    println!("✓ Seeded website language in database: {}", cfg.language);
    Ok(())
}

fn write_config(cfg: &InstallConfig) -> Result<()> {
    let text = render_maddy_conf(cfg);
    if let Some(parent) = cfg.config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ChatmailError::config(format!("mkdir {}: {e}", parent.display())))?;
    }
    std::fs::write(&cfg.config_path, text)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", cfg.config_path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cfg.config_path, std::fs::Permissions::from_mode(0o644))
            .map_err(|e| ChatmailError::config(format!("chmod config: {e}")))?;
    }
    println!("✓ Wrote {}", cfg.config_path.display());
    Ok(())
}

fn ensure_secrets(cfg: &mut InstallConfig) -> Result<()> {
    ensure_ss_password(cfg)?;
    if cfg.enable_turn && cfg.turn_secret.is_empty() {
        let mut b = [0u8; 16];
        getrandom::fill(&mut b).map_err(|e| ChatmailError::config(format!("turn secret: {e}")))?;
        cfg.turn_secret = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            b,
        );
    }
    Ok(())
}

fn print_next_steps(cfg: &InstallConfig) {
    println!("\nNext steps:");
    if cfg.tls_mode == "autocert" {
        if is_valid_ip_for_acme(&cfg.public_ip) {
            println!("  • Renew IP cert (daily): madmail certificate get");
        } else {
            println!("  • Renew: madmail certificate get");
        }
    }
    if !cfg.skip_systemd {
        println!(
            "  • Start:  systemctl reset-failed {} ; systemctl enable --now {}",
            cfg.binary_name, cfg.binary_name
        );
        println!(
            "  • Logs:   journalctl -u {} -n 100 --no-pager",
            cfg.binary_name
        );
    }
    println!("  • Admin:  madmail admin-token");
    if cfg.tls_mode == "self_signed" || cfg.turn_off_tls {
        println!("  • Delta Chat: use turn_off_tls / accept self-signed certs for IP relays");
    }
}

impl InstallConfig {
    fn from_args(global: &Args, args: &InstallArgs) -> Result<Self> {
        let binary_name = std::env::args()
            .next()
            .and_then(|p| {
                std::path::Path::new(&p)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "madmail".into());

        let config_dir = args.config_dir.clone();
        let state_dir = resolve_install_state_dir(&binary_name, global, args);
        let config_path = config_dir.join(format!("{binary_name}.conf"));
        let cert_dir = config_dir.join("certs");
        let cert_path = args
            .cert_path
            .clone()
            .unwrap_or_else(|| cert_dir.join("fullchain.pem"));
        let key_path = args
            .key_path
            .clone()
            .unwrap_or_else(|| cert_dir.join("privkey.pem"));

        let system_install = args.simple
            || config_dir.starts_with("/etc/")
            || state_dir.starts_with("/var/lib/");

        let binary_path = args
            .binary_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("/usr/local/bin/{binary_name}")));

        let (primary_domain, hostname, public_ip, local_domains, turn_off_tls) = if args.simple {
            let ip = args
                .ip
                .clone()
                .or_else(|| args.domain.clone())
                .ok_or_else(|| {
                    ChatmailError::config("--ip is required for --simple install")
                })?;
            let bare = ip.trim().trim_matches(|c| c == '[' || c == ']').to_string();
            let is_ip = bare.parse::<std::net::IpAddr>().is_ok();
            if !is_ip {
                return Err(ChatmailError::config(format!(
                    "--simple --ip requires an IPv4/IPv6 address, got {ip:?}"
                )));
            }

            let wrapped = wrap_ip_domain(&bare);
            let hostname = if args.domain.is_some() {
                wrap_ip_domain(args.hostname.as_deref().unwrap_or(&bare))
            } else {
                bare.clone()
            };
            let local_domains = if args.domain.is_some() {
                format!(
                    "$(primary_domain) {} [{}]",
                    bare, bare
                )
            } else {
                local_domains_for_ip(&bare)
            };

            (
                wrapped,
                hostname,
                bare,
                local_domains,
                args.turn_off_tls || (is_ip && !args.auto_ip_cert),
            )
        } else {
            let domain = args
                .domain
                .clone()
                .ok_or_else(|| ChatmailError::config("--domain is required (or use --simple --ip)"))?;
            let hostname = args
                .hostname
                .clone()
                .map(|h| wrap_ip_domain(&h))
                .unwrap_or_else(|| wrap_ip_domain(&domain));
            let public_ip = args
                .ip
                .clone()
                .unwrap_or_else(|| domain.trim_matches(|c| c == '[' || c == ']').to_string());
            (
                wrap_ip_domain(&domain),
                hostname,
                public_ip,
                "$(primary_domain)".into(),
                args.turn_off_tls,
            )
        };

        let enable_ss = args.enable_ss || args.simple;
        let enable_turn = true;
        let language = validate_language_code(&args.lang)?;

        Ok(Self {
            binary_name: binary_name.clone(),
            binary_path,
            maddy_user: binary_name.clone(),
            maddy_group: binary_name.clone(),
            hostname,
            primary_domain,
            local_domains,
            state_dir,
            runtime_dir: format!("/run/{binary_name}"),
            public_ip,
            tls_mode: args.tls_mode.clone().unwrap_or_default(),
            cert_path,
            key_path,
            acme_email: args.acme_email.clone().unwrap_or_default(),
            generate_certs: false,
            turn_off_tls,
            enable_chatmail: args.enable_chatmail || args.simple,
            enable_contact_sharing: true,
            enable_ss,
            enable_turn,
            turn_port: "3478".into(),
            turn_secret: String::new(),
            turn_ttl: 86400,
            ss_addr: "0.0.0.0:8388".into(),
            ss_password: String::new(),
            ss_cipher: "aes-128-gcm".into(),
            language,
            config_path,
            system_install,
            skip_user: args.skip_user,
            skip_systemd: args.skip_systemd,
            generated: chrono_like_now(),
        })
    }
}

fn ensure_ss_password(cfg: &mut InstallConfig) -> Result<()> {
    if !cfg.enable_ss || !cfg.ss_password.is_empty() {
        return Ok(());
    }
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|e| ChatmailError::config(format!("ss password: {e}")))?;
    cfg.ss_password = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        b,
    );
    Ok(())
}

/// Madmail `install.go` always uses `/var/lib/<binary>` for production installs, never cwd `./data`.
fn resolve_install_state_dir(binary_name: &str, global: &Args, args: &InstallArgs) -> PathBuf {
    if let Some(dir) = &args.state_dir {
        return dir.clone();
    }
    if args.simple || args.config_dir.starts_with("/etc/") {
        return PathBuf::from(format!("/var/lib/{binary_name}"));
    }
    if global.state_dir.is_relative() || is_local_dev_state_dir(&global.state_dir) {
        return PathBuf::from(format!("/var/lib/{binary_name}"));
    }
    global.state_dir.clone()
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_config::Args;

    /// Globally routable example IP for `--auto-ip-cert` (not a real deployment host).
    /// RFC 5737 addresses such as 203.0.113.50 are documentation-only and are rejected by ACME.
    const EXAMPLE_PUBLIC_IP: &str = "1.2.3.4";

    #[test]
    fn simple_ip_install_config_matches_madmail() {
        let global = Args {
            config: PathBuf::from("/etc/madmail/madmail.conf"),
            state_dir: PathBuf::from("./data"),
            boot_once: false,
        };
        let args = InstallArgs {
            non_interactive: false,
            simple: true,
            domain: None,
            hostname: None,
            ip: Some(EXAMPLE_PUBLIC_IP.into()),
            config_dir: PathBuf::from("/etc/madmail"),
            state_dir: None,
            tls_mode: None,
            cert_path: None,
            key_path: None,
            acme_email: None,
            enable_chatmail: false,
            enable_ss: false,
            turn_off_tls: false,
            dry_run: false,
            skip_systemd: false,
            skip_user: false,
            binary_path: None,
            obtain_certificate: true,
            auto_ip_cert: false,
            lang: "en".into(),
        };
        let cfg = InstallConfig::from_args(&global, &args).unwrap();
        assert_eq!(cfg.primary_domain, format!("[{EXAMPLE_PUBLIC_IP}]"));
        assert_eq!(cfg.hostname, EXAMPLE_PUBLIC_IP);
        assert_eq!(cfg.public_ip, EXAMPLE_PUBLIC_IP);
        assert!(cfg.local_domains.contains(&format!("[{EXAMPLE_PUBLIC_IP}]")));
        assert!(cfg.turn_off_tls);
        let mut args_ip_cert = args.clone();
        args_ip_cert.auto_ip_cert = true;
        args_ip_cert.acme_email = Some("admin@example.com".into());
        let mut cfg_ip = InstallConfig::from_args(&global, &args_ip_cert).unwrap();
        resolve_tls_mode(&mut cfg_ip, &args_ip_cert).unwrap();
        assert_eq!(cfg_ip.tls_mode, "autocert");
        assert!(!cfg_ip.turn_off_tls);
        assert!(cfg.enable_ss);
        assert!(cfg.enable_turn);
        assert!(cfg.cert_path.starts_with("/etc/madmail/certs"));
        assert!(
            cfg.state_dir.starts_with("/var/lib/"),
            "state_dir {:?}",
            cfg.state_dir
        );
        assert!(!cfg.state_dir.to_string_lossy().contains("./data"));
        assert!(cfg.system_install);
        assert_eq!(cfg.language, "en");
    }

    #[test]
    fn install_lang_flag_sets_config_language() {
        let global = Args {
            config: PathBuf::from("/etc/madmail/madmail.conf"),
            state_dir: PathBuf::from("./data"),
            boot_once: false,
        };
        let args = InstallArgs {
            lang: "fa".into(),
            ..InstallArgs {
                non_interactive: false,
                simple: true,
                domain: None,
                hostname: None,
                ip: Some(EXAMPLE_PUBLIC_IP.into()),
                config_dir: PathBuf::from("/etc/madmail"),
                state_dir: None,
                tls_mode: None,
                cert_path: None,
                key_path: None,
                acme_email: None,
                enable_chatmail: false,
                enable_ss: false,
                turn_off_tls: false,
                dry_run: false,
                skip_systemd: false,
                skip_user: false,
                binary_path: None,
                obtain_certificate: true,
                auto_ip_cert: false,
                lang: "en".into(),
            }
        };
        let cfg = InstallConfig::from_args(&global, &args).unwrap();
        assert_eq!(cfg.language, "fa");
        let conf = render_maddy_conf(&cfg);
        assert!(conf.contains("language fa"));
    }

    #[test]
    fn install_rejects_unknown_lang() {
        let global = Args {
            config: PathBuf::from("/etc/madmail/madmail.conf"),
            state_dir: PathBuf::from("./data"),
            boot_once: false,
        };
        let args = InstallArgs {
            lang: "de".into(),
            ..InstallArgs {
                non_interactive: false,
                simple: true,
                domain: None,
                hostname: None,
                ip: Some(EXAMPLE_PUBLIC_IP.into()),
                config_dir: PathBuf::from("/etc/madmail"),
                state_dir: None,
                tls_mode: None,
                cert_path: None,
                key_path: None,
                acme_email: None,
                enable_chatmail: false,
                enable_ss: false,
                turn_off_tls: false,
                dry_run: false,
                skip_systemd: false,
                skip_user: false,
                binary_path: None,
                obtain_certificate: true,
                auto_ip_cert: false,
                lang: "en".into(),
            }
        };
        assert!(InstallConfig::from_args(&global, &args).is_err());
    }
}
