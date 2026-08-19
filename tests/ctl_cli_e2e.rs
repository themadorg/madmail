//! End-to-end tests: `chatmail` binary CLI (accounts, blocklist, delete, ban-list).

use std::process::Command;

use chatmail_config::{effective_app_db_path, AppConfig};
use chatmail_db::{blocklist, init_db, passwords};
use chatmail_integration::chatmail_bin;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn chatmail() -> assert_cmd::Command {
    Command::new(chatmail_bin()).into()
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
fn e2e_ctl_accounts_status_json() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let mut base = state_argv(&state);
    base.push("--json".into());

    let out = chatmail()
        .args(base)
        .arg("accounts")
        .arg("status")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let envelope: Value = serde_json::from_slice(&out).expect("accounts status --json stdout");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "accounts status");
    let data = &envelope["data"];
    assert!(data["login_count"].is_number());
    assert!(data["registration_open"].is_boolean());
    assert!(data["token_required"].is_boolean());
    assert!(data["jit_enabled"].is_boolean());
    assert!(data["blocklisted"].is_number());
    assert!(data["mail_directories"].is_number());
    assert!(data["state_dir"].is_string());
    assert!(data["database"].is_string());
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
    status
        .assert()
        .success()
        .stdout(predicate::str::contains("Login accounts:"));

    let mut create = chatmail();
    create.args(base.clone());
    create.args(["create-user", "--json-only"]);
    let create_out = create.assert().success().get_output().stdout.clone();
    let creds: Value = serde_json::from_slice(&create_out).expect("create-user JSON stdout");
    let dclogin = creds["dclogin"].as_str().expect("dclogin field");
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
    top_ban
        .assert()
        .success()
        .stdout(predicate::str::contains(&email));
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
        .args(["accounts", "create", email, "--password", "export-pass-99"])
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
    assert!(entries
        .iter()
        .any(|e| e["username"].as_str() == Some(email)));

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

/// Old `maddy update` (through 2.20.0) chmods the live path to 0700 then execs
/// `version` as root. The new binary must restore 0755 (GitHub #147).
#[cfg(unix)]
#[test]
fn e2e_version_repairs_0700_live_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().expect("tempdir");
    let dest = dir.path().join("maddy");
    std::fs::copy(chatmail_bin(), &dest).expect("copy madmail");
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777,
        0o700
    );

    let out = Command::new(&dest)
        .arg("version")
        .output()
        .expect("run version");
    assert!(
        out.status.success(),
        "version failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("madmail-v2"),
        "expected version banner, got: {stdout}"
    );

    let mode = std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o755,
        "version must restore 0755 after old-updater 0700 (#147), got {mode:#o}"
    );
}

/// Same heal when the service PATH entry is a symlink into the version tree.
#[cfg(unix)]
#[test]
fn e2e_version_repairs_0700_through_symlink() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().expect("tempdir");
    let target = dir.path().join("madmail");
    let link = dir.path().join("maddy");
    std::fs::copy(chatmail_bin(), &target).expect("copy madmail");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let out = Command::new(&link)
        .arg("version")
        .output()
        .expect("run version via symlink");
    assert!(
        out.status.success(),
        "version failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o755,
        "version must chmod symlink target to 0755 (#147), got {mode:#o}"
    );
}

#[test]
fn e2e_ctl_dkim_show_json() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path();
    let config = state.join("madmail.conf");
    std::fs::write(
        &config,
        "hostname mail.example.org\n$(primary_domain) = example.org\n",
    )
    .expect("write dkim e2e config");

    let out = chatmail()
        .args([
            "--state-dir",
            state.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
            "dkim",
            "show",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let envelope: Value = serde_json::from_slice(&out).expect("dkim show --json stdout");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "dkim show");
    let data = &envelope["data"];
    assert_eq!(data["selector"], "default");
    assert_eq!(data["domain"], "example.org");
    assert_eq!(data["dns_name"], "default._domainkey");
    assert_eq!(data["dns_fqdn"], "default._domainkey.example.org");
    assert_eq!(data["publishable"], true);
    assert_eq!(data["key_present"], true);
    assert_eq!(data["generated"], true);
    let txt = data["txt"].as_str().expect("txt");
    assert!(
        txt.starts_with("v=DKIM1; k=rsa; p="),
        "unexpected TXT: {txt}"
    );
    assert!(
        !txt.contains('\n'),
        "TXT must be a single line for DNS publish"
    );
    let private = data["private_key_path"].as_str().expect("private_key_path");
    let txt_path = data["txt_path"].as_str().expect("txt_path");
    assert!(private.ends_with("dkim/default.private"));
    assert!(txt_path.ends_with("dkim/default.txt"));
    assert!(std::path::Path::new(private).is_file());
    assert!(std::path::Path::new(txt_path).is_file());
}

#[test]
fn e2e_ctl_dkim_check_json_skips_ip() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path();
    let config = state.join("madmail.conf");
    std::fs::write(&config, "hostname 203.0.113.10\n$(primary_domain) = 203.0.113.10\n")
        .expect("write dkim check config");

    let out = chatmail()
        .args([
            "--state-dir",
            state.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
            "dkim",
            "check",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let envelope: Value = serde_json::from_slice(&out).expect("dkim check --json stdout");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "dkim check");
    let data = &envelope["data"];
    assert_eq!(data["checked"], false);
    assert_eq!(data["matched"], false);
    assert!(data["reason"].as_str().unwrap().contains("IP literal"));
}

#[test]
fn e2e_ctl_dkim_status_json_missing_key() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path();
    let config = state.join("madmail.conf");
    std::fs::write(
        &config,
        "hostname mail.example.org\n$(primary_domain) = example.org\n",
    )
    .expect("write dkim status config");

    let out = chatmail()
        .args([
            "--state-dir",
            state.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
            "dkim",
            "status",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let envelope: Value = serde_json::from_slice(&out).expect("dkim status --json stdout");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "dkim status");
    let data = &envelope["data"];
    assert_eq!(data["selector"], "default");
    assert_eq!(data["domain"], "example.org");
    assert_eq!(data["key_present"], false);
    assert_eq!(data["publishable"], false);
    assert_eq!(data["dns_checked"], false);
    assert_eq!(data["generated"], false);
    assert!(!state.join("dkim/default.private").is_file());
}
