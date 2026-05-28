//! End-to-end tests: `chatmail` binary CLI (accounts, blocklist, delete, ban-list).

use std::process::Command;

use assert_cmd::cargo::cargo_bin;
use chatmail_config::{effective_app_db_path, AppConfig};
use chatmail_db::{blocklist, init_db, passwords};
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn chatmail() -> assert_cmd::Command {
    Command::new(cargo_bin("chatmail")).into()
}

fn state_argv(state_dir: &str) -> Vec<String> {
    vec![
        "--state-dir".into(),
        state_dir.into(),
        "--config".into(),
        format!("{state_dir}/_e2e_no_config_.conf"),
    ]
}

#[test]
fn e2e_ctl_accounts_create_random_delete_and_ban_list() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let base = state_argv(&state);

    // Warm up DB (same as operator first touch).
    let mut status = chatmail();
    status.args(base.clone());
    status.arg("accounts").arg("status");
    status.assert().success().stdout(predicate::str::contains("Login accounts:"));

    let mut create = chatmail();
    create.args(base.clone());
    create.args(["create-user", "--json-only"]);
    let create_out = create.assert().success().get_output().stdout.clone();
    let creds: Value =
        serde_json::from_slice(&create_out).expect("create-user JSON stdout");
    let dclogin = creds["dclogin"]
        .as_str()
        .expect("dclogin field");
    let email = dclogin
        .strip_prefix("dclogin:")
        .and_then(|s| s.split_once("/?"))
        .map(|(e, _)| e.to_string())
        .expect("email in dclogin URI");

    let rt = tokio::runtime::Runtime::new().unwrap();
    let db_path = effective_app_db_path(dir.path(), &AppConfig::default());
    rt.block_on(async {
        let pool = init_db(&db_path).await.expect("db");
        assert!(passwords::user_exists(&pool, &email).await.unwrap());
    });

    let mut del = chatmail();
    del.args(base.clone());
    del.args(["accounts", "delete", &email, "-y"]);
    del.assert()
        .success()
        .stdout(predicate::str::contains("Deleted and blocklisted"));

    rt.block_on(async {
        let pool = init_db(&db_path).await.expect("db");
        assert!(!passwords::user_exists(&pool, &email).await.unwrap());
        assert!(blocklist::is_blocked(&pool, &email).await.unwrap());
    });

    let mut ban_list = chatmail();
    ban_list.args(base.clone());
    ban_list.arg("ban-list");
    ban_list
        .assert()
        .success()
        .stdout(predicate::str::contains(email.as_str()));

    let mut top_ban = chatmail();
    top_ban.args(base);
    top_ban.arg("ban-list");
    top_ban.assert().success().stdout(predicate::str::contains(&email));
}

#[test]
fn e2e_ctl_blocklist_add_remove() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let base = state_argv(&state);
    let user = "blockme@example.org";

    chatmail()
        .args(base.clone())
        .args(["blocklist", "add", user, "e2e block"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Blocked"));

    let db_path = effective_app_db_path(dir.path(), &AppConfig::default());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = init_db(&db_path).await.unwrap();
        assert!(blocklist::is_blocked(&pool, user).await.unwrap());
    });

    chatmail()
        .args(base.clone())
        .args(["blocklist", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(user));

    chatmail()
        .args(base)
        .args(["blocklist", "remove", user, "-y"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Unblocked"));

    rt.block_on(async {
        let pool = init_db(&db_path).await.unwrap();
        assert!(!blocklist::is_blocked(&pool, user).await.unwrap());
    });
}

#[test]
fn e2e_ctl_delete_top_level_with_custom_reason() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let base = state_argv(&state);
    let email = "topdel@example.org";

    chatmail()
        .args(base.clone())
        .args([
            "accounts",
            "create",
            email,
            "--password",
            "topdel-e2e-pass-99",
        ])
        .assert()
        .success();

    chatmail()
        .args(base.clone())
        .args(["delete", email, "-y", "--reason", "e2e gone"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted and blocklisted"));

    let db_path = effective_app_db_path(dir.path(), &AppConfig::default());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = init_db(&db_path).await.unwrap();
        assert!(!passwords::user_exists(&pool, email).await.unwrap());
        let rows = blocklist::list_blocked_users(&pool).await.unwrap();
        assert!(rows.iter().any(|(u, r, _)| u == email && r == "e2e gone"));
    });
}

#[test]
fn e2e_ctl_accounts_export_import() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let base = state_argv(&state);
    let email = "export@example.org";
    let export_file = dir.path().join("exported.json");
    let export_s = export_file.to_str().unwrap();

    chatmail()
        .args(base.clone())
        .args([
            "accounts",
            "create",
            email,
            "--password",
            "export-pass-99",
        ])
        .assert()
        .success();

    chatmail()
        .args(base.clone())
        .args(["accounts", "export", "-o", export_s])
        .assert()
        .success()
        .stdout(predicate::str::contains("Exported"));

    assert!(export_file.is_file());
    let raw = std::fs::read_to_string(&export_file).unwrap();
    let entries: Vec<Value> = serde_json::from_str(&raw).unwrap();
    assert!(entries.iter().any(|e| e["username"].as_str() == Some(email)));

    chatmail()
        .args(base.clone())
        .args(["accounts", "delete", email, "-y"])
        .assert()
        .success();

    let db_path = effective_app_db_path(dir.path(), &AppConfig::default());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = init_db(&db_path).await.unwrap();
        blocklist::unblock_user(&pool, email).await.unwrap();
    });

    chatmail()
        .args(base)
        .args(["accounts", "import", export_s])
        .assert()
        .success()
        .stdout(predicate::str::contains("Imported: 1"));

    rt.block_on(async {
        let pool = init_db(&db_path).await.unwrap();
        assert!(passwords::user_exists(&pool, email).await.unwrap());
    });
}
