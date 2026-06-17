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

//! In-process tests for operator CLI commands.

use chatmail_config::cli::{
    EndpointCacheCommand, FederationCommand, LanguageCommand, PortCommand, PortServiceCommand,
    ProxyCommand, ProxySettingCommand, RegistrationCommand, RegistrationTokensCommand,
    ServiceToggleCommand, SharingCommand,
};
use chatmail_config::{Cli, Command};
use chatmail_db::{
    federation_policy_label, get_bool_setting, get_endpoint_override, get_setting, init_sharing_db,
    list_sharing_contacts, settings_keys,
};
use clap::Parser;

use super::dispatch;
use super::test_harness::{parse_cli, parse_cli_with_config, setup_ctl_env, write_ss_test_config};

#[tokio::test]
async fn dispatch_registration_open_close() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;

    let cli = parse_cli(dir.path(), &["registration", "close"]);
    dispatch(&cli).await.unwrap();
    assert!(
        !get_bool_setting(&pool, settings_keys::REGISTRATION_OPEN, true)
            .await
            .unwrap()
    );

    let cli = parse_cli(dir.path(), &["registration", "open"]);
    dispatch(&cli).await.unwrap();
    assert!(
        get_bool_setting(&pool, settings_keys::REGISTRATION_OPEN, false)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn dispatch_language_set_and_reset() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;

    let cli = parse_cli(dir.path(), &["language", "set", "fa"]);
    dispatch(&cli).await.unwrap();
    assert_eq!(
        get_setting(&pool, settings_keys::LANGUAGE)
            .await
            .unwrap()
            .as_deref(),
        Some("fa")
    );

    let cli = parse_cli(dir.path(), &["language", "reset"]);
    dispatch(&cli).await.unwrap();
    assert!(get_setting(&pool, settings_keys::LANGUAGE)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn dispatch_webimap_websmtp_toggle() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;

    let cli = parse_cli(dir.path(), &["webimap", "enable"]);
    dispatch(&cli).await.unwrap();
    assert!(
        get_bool_setting(&pool, settings_keys::WEBIMAP_ENABLED, false)
            .await
            .unwrap()
    );

    let cli = parse_cli(dir.path(), &["websmtp", "disable"]);
    dispatch(&cli).await.unwrap();
    assert!(
        !get_bool_setting(&pool, settings_keys::WEBSMTP_ENABLED, true)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn dispatch_html_export() {
    let (dir, _args, _db, _pool) = setup_ctl_env().await;
    let dest = dir.path().join("exported-www");
    let dest_s = dest.to_str().unwrap();
    let cli = parse_cli(dir.path(), &["html-export", dest_s]);
    dispatch(&cli).await.unwrap();
    assert!(dest.join("index.html").is_file());
}

#[tokio::test]
async fn dispatch_federation_policy_and_rules() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;

    let cli = parse_cli(dir.path(), &["federation", "policy", "reject"]);
    dispatch(&cli).await.unwrap();
    assert_eq!(federation_policy_label(&pool).await.unwrap(), "REJECT");

    let cli = parse_cli(dir.path(), &["federation", "block", "evil.example"]);
    dispatch(&cli).await.unwrap();

    let cli = parse_cli(dir.path(), &["federation", "list"]);
    dispatch(&cli).await.unwrap();

    let cli = parse_cli(dir.path(), &["federation", "remove", "evil.example"]);
    dispatch(&cli).await.unwrap();
}

#[tokio::test]
async fn dispatch_federation_silent_dismiss() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;

    let cli = parse_cli(dir.path(), &["federation", "dismiss", "1.1.1.1"]);
    dispatch(&cli).await.unwrap();

    let count: i64 = chatmail_db::db_fetch_scalar!(
        &pool,
        i64,
        "SELECT COUNT(*) FROM federation_silent_dismiss WHERE domain = '1.1.1.1'"
    )
    .unwrap();
    assert_eq!(count, 1);

    let cli = parse_cli(dir.path(), &["federation", "dismiss-list"]);
    dispatch(&cli).await.unwrap();

    let cli = parse_cli(dir.path(), &["federation", "undismiss", "1.1.1.1"]);
    dispatch(&cli).await.unwrap();

    let count: i64 = chatmail_db::db_fetch_scalar!(
        &pool,
        i64,
        "SELECT COUNT(*) FROM federation_silent_dismiss WHERE domain = '1.1.1.1'"
    )
    .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn dispatch_registration_tokens_create_and_delete() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;

    let cli = parse_cli(
        dir.path(),
        &[
            "registration-tokens",
            "create",
            "--token",
            "test-invite",
            "--max-uses",
            "3",
            "--comment",
            "cli-test",
        ],
    );
    dispatch(&cli).await.unwrap();

    let count: i64 = chatmail_db::db_fetch_scalar!(
        &pool,
        i64,
        "SELECT COUNT(*) FROM registration_tokens WHERE token = 'test-invite'"
    )
    .unwrap();
    assert_eq!(count, 1);

    let cli = parse_cli(dir.path(), &["registration-tokens", "list"]);
    dispatch(&cli).await.unwrap();

    let cli = parse_cli(
        dir.path(),
        &["registration-tokens", "delete", "test-invite"],
    );
    dispatch(&cli).await.unwrap();
}

#[tokio::test]
async fn dispatch_sharing_create_and_remove() {
    let (dir, _args, _db, _pool) = setup_ctl_env().await;
    let sharing_db = dir.path().join("sharing.db");

    let cli = parse_cli(
        dir.path(),
        &[
            "sharing",
            "create",
            "bob",
            "https://i.delta.chat/#ABCDEF",
            "Bob",
        ],
    );
    dispatch(&cli).await.unwrap();
    assert!(sharing_db.is_file());

    let pool = init_sharing_db(&sharing_db).await.unwrap();
    let rows = list_sharing_contacts(&pool).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].slug, "bob");

    let cli = parse_cli(dir.path(), &["sharing", "remove", "bob"]);
    dispatch(&cli).await.unwrap();
    assert!(list_sharing_contacts(&pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn dispatch_proxy_not_configured_status_and_enable_guard() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;

    let cli = parse_cli(dir.path(), &["proxy", "status"]);
    dispatch(&cli).await.unwrap();

    let cli = parse_cli(dir.path(), &["proxy", "disable"]);
    dispatch(&cli).await.unwrap();
    assert!(!get_bool_setting(&pool, settings_keys::SS_ENABLED, true)
        .await
        .unwrap());

    let cli = parse_cli(dir.path(), &["proxy", "enable"]);
    let err = dispatch(&cli).await.unwrap_err().to_string();
    assert!(err.contains("not configured"));
}

#[tokio::test]
async fn dispatch_proxy_configured_toggle_and_settings() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;
    let config = write_ss_test_config(dir.path());

    let cli = parse_cli_with_config(dir.path(), &config, &["proxy", "status"]);
    dispatch(&cli).await.unwrap();

    let cli = parse_cli_with_config(dir.path(), &config, &["proxy", "disable"]);
    dispatch(&cli).await.unwrap();
    assert!(!get_bool_setting(&pool, settings_keys::SS_ENABLED, true)
        .await
        .unwrap());

    let cli = parse_cli_with_config(dir.path(), &config, &["proxy", "enable"]);
    dispatch(&cli).await.unwrap();
    assert!(get_bool_setting(&pool, settings_keys::SS_ENABLED, false)
        .await
        .unwrap());

    let cli = parse_cli_with_config(
        dir.path(),
        &config,
        &["proxy", "cipher", "set", "chacha20-ietf-poly1305"],
    );
    dispatch(&cli).await.unwrap();
    assert_eq!(
        get_setting(&pool, settings_keys::SS_CIPHER)
            .await
            .unwrap()
            .as_deref(),
        Some("chacha20-ietf-poly1305")
    );

    let cli = parse_cli_with_config(dir.path(), &config, &["proxy", "cipher", "reset"]);
    dispatch(&cli).await.unwrap();
    assert!(get_setting(&pool, settings_keys::SS_CIPHER)
        .await
        .unwrap()
        .is_none());

    let cli = parse_cli_with_config(
        dir.path(),
        &config,
        &["proxy", "password", "set", "new-pass"],
    );
    dispatch(&cli).await.unwrap();
    assert_eq!(
        get_setting(&pool, settings_keys::SS_PASSWORD)
            .await
            .unwrap()
            .as_deref(),
        Some("new-pass")
    );

    let cli = parse_cli_with_config(dir.path(), &config, &["proxy", "password", "reset"]);
    dispatch(&cli).await.unwrap();
    assert!(get_setting(&pool, settings_keys::SS_PASSWORD)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn dispatch_proxy_cipher_rejects_invalid() {
    let (dir, _args, _db, _pool) = setup_ctl_env().await;
    let config = write_ss_test_config(dir.path());

    let cli = parse_cli_with_config(dir.path(), &config, &["proxy", "cipher", "set", "rc4-md5"]);
    let err = dispatch(&cli).await.unwrap_err().to_string();
    assert!(err.contains("unsupported shadowsocks cipher"));
}

#[tokio::test]
async fn dispatch_port_set_and_reset() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;

    let cli = parse_cli(dir.path(), &["port", "imap", "set", "1143"]);
    dispatch(&cli).await.unwrap();
    assert_eq!(
        get_setting(&pool, settings_keys::IMAP_PORT)
            .await
            .unwrap()
            .as_deref(),
        Some("1143")
    );

    let cli = parse_cli(dir.path(), &["port", "imap", "reset"]);
    dispatch(&cli).await.unwrap();
    assert!(get_setting(&pool, settings_keys::IMAP_PORT)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn dispatch_endpoint_cache_crud() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;

    let cli = parse_cli(
        dir.path(),
        &[
            "endpoint-cache",
            "set",
            "mx.example.org",
            "127.0.0.1",
            "test",
        ],
    );
    dispatch(&cli).await.unwrap();
    let row = get_endpoint_override(&pool, "mx.example.org")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.target_host, "127.0.0.1");

    let cli = parse_cli(dir.path(), &["endpoint-cache", "get", "mx.example.org"]);
    dispatch(&cli).await.unwrap();

    let cli = parse_cli(dir.path(), &["endpoint-cache", "remove", "mx.example.org"]);
    dispatch(&cli).await.unwrap();
    assert!(get_endpoint_override(&pool, "mx.example.org")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn dispatch_status_shows_registered_users() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;
    chatmail_db::passwords::create_user(&pool, "u@x.org", "bcrypt:test")
        .await
        .unwrap();

    let cli = parse_cli(dir.path(), &["status"]);
    dispatch(&cli).await.unwrap();
}

#[test]
fn cli_language_and_registration_parse() {
    let cli = parse_cli(std::path::Path::new("/tmp"), &["language", "set", "ru"]);
    assert!(matches!(
        cli.command,
        Some(Command::Language {
            command: Some(LanguageCommand::Set { lang })
        }) if lang == "ru"
    ));

    let cli = parse_cli(std::path::Path::new("/tmp"), &["registration", "status"]);
    assert!(matches!(
        cli.command,
        Some(Command::Registration(RegistrationCommand::Status))
    ));

    let cli = parse_cli(std::path::Path::new("/tmp"), &["webimap", "disable"]);
    assert!(matches!(
        cli.command,
        Some(Command::Webimap(ServiceToggleCommand::Disable))
    ));

    let cli = parse_cli(
        std::path::Path::new("/tmp"),
        &["federation", "policy", "accept"],
    );
    assert!(matches!(
        cli.command,
        Some(Command::Federation(FederationCommand::Policy {
            policy
        })) if policy == "accept"
    ));

    let cli = parse_cli(
        std::path::Path::new("/tmp"),
        &["registration-tokens", "create", "--max-uses", "5"],
    );
    assert!(matches!(
        cli.command,
        Some(Command::RegistrationTokens(
            RegistrationTokensCommand::Create { max_uses: 5, .. }
        ))
    ));

    let cli = parse_cli(std::path::Path::new("/tmp"), &["sharing", "list"]);
    assert!(matches!(
        cli.command,
        Some(Command::Sharing(SharingCommand::List))
    ));

    let cli = Cli::try_parse_from(["chatmail", "status", "--details"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Status { details: true })
    ));

    let cli = parse_cli(
        std::path::Path::new("/tmp"),
        &["port", "https", "set", "8443"],
    );
    assert!(matches!(
        cli.command,
        Some(Command::Port(PortCommand::Https(PortServiceCommand::Set {
            port
        }))) if port == "8443"
    ));

    let cli = parse_cli(
        std::path::Path::new("/tmp"),
        &["endpoint-cache", "set", "a.example", "b.example"],
    );
    assert!(matches!(
        cli.command,
        Some(Command::EndpointCache(EndpointCacheCommand::Set {
            lookup_key,
            target_host,
            ..
        })) if lookup_key == "a.example" && target_host == "b.example"
    ));

    let cli = Cli::try_parse_from(["chatmail", "reload", "--insecure"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Reload { insecure: true, .. })
    ));

    let cli = parse_cli(std::path::Path::new("/tmp"), &["proxy", "status"]);
    assert!(matches!(
        cli.command,
        Some(Command::Proxy {
            cmd: Some(ProxyCommand::Status)
        })
    ));

    let cli = parse_cli(std::path::Path::new("/tmp"), &["proxy", "enable"]);
    assert!(matches!(
        cli.command,
        Some(Command::Proxy {
            cmd: Some(ProxyCommand::Enable)
        })
    ));

    let cli = parse_cli(std::path::Path::new("/tmp"), &["pr", "status"]);
    assert!(matches!(
        cli.command,
        Some(Command::Proxy {
            cmd: Some(ProxyCommand::Status)
        })
    ));

    let cli = parse_cli(
        std::path::Path::new("/tmp"),
        &["proxy", "cipher", "set", "aes-256-gcm"],
    );
    assert!(matches!(
        cli.command,
        Some(Command::Proxy {
            cmd: Some(ProxyCommand::Cipher {
                cmd: Some(ProxySettingCommand::Set { value })
            })
        }) if value == "aes-256-gcm"
    ));
}
