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

//! In-process `ctl::dispatch` tests (no subprocess).

use chatmail_config::Command;
use chatmail_db::{blocklist, passwords, CLI_BAN_REASON, CLI_DELETE_REASON};
use chatmail_storage::MailboxStore;

use super::dispatch;
use super::test_harness::{parse_cli, setup_ctl_env};

#[tokio::test]
async fn dispatch_accounts_create_delete_and_blocklist() {
    let (dir, _args, _db_path, pool) = setup_ctl_env().await;
    let email = "ctluser@example.org";
    let password = "testpass-ctl-99";

    let cli = parse_cli(
        dir.path(),
        &[
            "accounts",
            "create",
            email,
            "--password",
            password,
        ],
    );
    dispatch(&cli).await.expect("accounts create");

    assert!(passwords::user_exists(&pool, email).await.unwrap());
    let mailbox = MailboxStore::new(dir.path());
    assert!(mailbox.maildir_for_user(email).root.exists());

    let cli = parse_cli(dir.path(), &["accounts", "delete", email, "-y"]);
    dispatch(&cli).await.expect("accounts delete");

    assert!(!passwords::user_exists(&pool, email).await.unwrap());
    assert!(blocklist::is_blocked(&pool, email).await.unwrap());
    let rows = blocklist::list_blocked_users(&pool).await.unwrap();
    assert!(rows.iter().any(|(u, r, _)| u == email && r == CLI_DELETE_REASON));
}

#[tokio::test]
async fn dispatch_accounts_ban_uses_custom_reason() {
    let (dir, _args, _db_path, pool) = setup_ctl_env().await;
    let email = "banned@example.org";

    let cli = parse_cli(
        dir.path(),
        &[
            "accounts",
            "create",
            email,
            "--password",
            "ban-me-password-1",
        ],
    );
    dispatch(&cli).await.unwrap();

    let cli = parse_cli(
        dir.path(),
        &["accounts", "ban", email, "spam via test", "-y"],
    );
    dispatch(&cli).await.unwrap();

    let rows = blocklist::list_blocked_users(&pool).await.unwrap();
    assert!(rows.iter().any(|(u, r, _)| u == email && r == "spam via test"));
}

#[tokio::test]
async fn dispatch_accounts_unban_after_ban() {
    let (dir, _args, _db_path, pool) = setup_ctl_env().await;
    let email = "unban@example.org";

    blocklist::block_user(&pool, email, CLI_BAN_REASON)
        .await
        .unwrap();
    assert!(blocklist::is_blocked(&pool, email).await.unwrap());

    let cli = parse_cli(dir.path(), &["accounts", "unban", email, "-y"]);
    dispatch(&cli).await.unwrap();
    assert!(!blocklist::is_blocked(&pool, email).await.unwrap());
}

#[tokio::test]
async fn dispatch_create_user_json_only() {
    let (dir, _args, _db_path, pool) = setup_ctl_env().await;

    let cli = parse_cli(dir.path(), &["create-user", "--json-only"]);
    dispatch(&cli).await.expect("create-user");

    let users: Vec<String> = passwords::list_users(&pool).await.unwrap();
    let created = users
        .iter()
        .find(|u| u.ends_with("@127.0.0.1") || u.contains('@'))
        .expect("random account in db");
    assert!(passwords::user_exists(&pool, created).await.unwrap());
}

#[tokio::test]
async fn dispatch_blocklist_add_and_remove() {
    let (dir, _args, _db_path, pool) = setup_ctl_env().await;
    let user = "blocked@example.org";

    let cli = parse_cli(
        dir.path(),
        &["blocklist", "add", user, "manual block test"],
    );
    dispatch(&cli).await.unwrap();
    assert!(blocklist::is_blocked(&pool, user).await.unwrap());

    let cli = parse_cli(dir.path(), &["blocklist", "remove", user, "-y"]);
    dispatch(&cli).await.unwrap();
    assert!(!blocklist::is_blocked(&pool, user).await.unwrap());
}

#[tokio::test]
async fn dispatch_accounts_export_import_roundtrip() {
    let (dir, _args, _db_path, pool) = setup_ctl_env().await;
    let email = "roundtrip@example.org";

    let cli = parse_cli(
        dir.path(),
        &[
            "accounts",
            "create",
            email,
            "--password",
            "roundtrip-pass-99",
        ],
    );
    dispatch(&cli).await.unwrap();

    let export_path = dir.path().join("accounts.json");
    let export_s = export_path.to_str().unwrap();
    let cli = parse_cli(
        dir.path(),
        &["accounts", "export", "-o", export_s],
    );
    dispatch(&cli).await.unwrap();
    assert!(export_path.is_file());

    let cli = parse_cli(dir.path(), &["accounts", "delete", email, "-y"]);
    dispatch(&cli).await.unwrap();
    assert!(!passwords::user_exists(&pool, email).await.unwrap());

    blocklist::unblock_user(&pool, email).await.unwrap();

    let import_s = export_path.to_str().unwrap();
    let cli = parse_cli(
        dir.path(),
        &["accounts", "import", import_s],
    );
    dispatch(&cli).await.unwrap();
    assert!(passwords::user_exists(&pool, email).await.unwrap());
}

#[tokio::test]
async fn dispatch_ban_list_top_level() {
    let (dir, _args, _db_path, pool) = setup_ctl_env().await;
    blocklist::block_user(&pool, "listed@example.org", "listed")
        .await
        .unwrap();

    let cli = parse_cli(dir.path(), &["ban-list"]);
    dispatch(&cli).await.expect("ban-list");
}

#[tokio::test]
async fn dispatch_delete_top_level_matches_accounts_delete() {
    let (dir, _args, _db_path, pool) = setup_ctl_env().await;
    let email = "topdel@example.org";

    let cli = parse_cli(
        dir.path(),
        &[
            "accounts",
            "create",
            email,
            "--password",
            "topdel-pass-99",
        ],
    );
    dispatch(&cli).await.unwrap();

    let cli = parse_cli(dir.path(), &["delete", email, "-y", "--reason", "gone"]);
    dispatch(&cli).await.unwrap();

    assert!(!passwords::user_exists(&pool, email).await.unwrap());
    assert!(blocklist::is_blocked(&pool, email).await.unwrap());
    let rows = blocklist::list_blocked_users(&pool).await.unwrap();
    assert!(rows.iter().any(|(u, r, _)| u == email && r == "gone"));
}

#[test]
fn accounts_and_blocklist_subcommands_parse() {
    use chatmail_config::cli::{AccountsCommand, BlocklistCommand};

    let cli = parse_cli(
        std::path::Path::new("/tmp"),
        &["accounts", "create-random", "--json-only"],
    );
    assert!(matches!(
        cli.command,
        Some(Command::Accounts(AccountsCommand::CreateRandom { json_only: true }))
    ));

    let cli = parse_cli(
        std::path::Path::new("/tmp"),
        &["blocklist", "remove", "u@x.org", "-y"],
    );
    assert!(matches!(
        cli.command,
        Some(Command::Blocklist(BlocklistCommand::Remove {
            username,
            yes: true,
        })) if username == "u@x.org"
    ));
}
