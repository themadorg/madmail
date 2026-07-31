//! E2E: language, registration, webimap, html-export, openrelay.

use std::process::Command;

use chatmail_config::effective_app_db_path;
use chatmail_config::AppConfig;
use chatmail_db::{get_bool_setting, get_setting, init_db, settings_keys};
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
fn e2e_registration_and_webimap() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let base = state_argv(&state);

    chatmail()
        .args(base.clone())
        .args(["registration", "close"])
        .assert()
        .success()
        .stdout(predicate::str::contains("CLOSED"));

    chatmail()
        .args(base.clone())
        .args(["webimap", "enable"])
        .assert()
        .success()
        .stdout(predicate::str::contains("enabled"));

    let db_path = effective_app_db_path(dir.path(), &AppConfig::default());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = init_db(&db_path).await.unwrap();
        assert!(
            !get_bool_setting(&pool, settings_keys::REGISTRATION_OPEN, true)
                .await
                .unwrap()
        );
        assert!(
            get_bool_setting(&pool, settings_keys::WEBIMAP_ENABLED, false)
                .await
                .unwrap()
        );
    });
}

#[test]
fn e2e_webmail_cors_enable() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let base = state_argv(&state);

    chatmail()
        .args(base.clone())
        .args(["webmail-cors", "enable", "http://127.0.0.1:5173"])
        .assert()
        .success()
        .stdout(predicate::str::contains("127.0.0.1:5173"));

    let db_path = effective_app_db_path(dir.path(), &AppConfig::default());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = init_db(&db_path).await.unwrap();
        assert!(
            get_bool_setting(&pool, settings_keys::WEBIMAP_ENABLED, false)
                .await
                .unwrap()
        );
        assert!(
            get_bool_setting(&pool, settings_keys::WEBSMTP_ENABLED, false)
                .await
                .unwrap()
        );
        let cors = get_setting(&pool, settings_keys::WEBMAIL_CORS_ORIGINS)
            .await
            .unwrap()
            .unwrap_or_default();
        assert!(cors.contains("http://127.0.0.1:5173"));
    });
}

#[test]
fn e2e_webmail_cors_enable_disable_no_origin() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let base = state_argv(&state);

    chatmail()
        .args(base.clone())
        .args(["webmail-cors", "enable"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Browser access enabled"));

    chatmail()
        .args(base.clone())
        .args(["webmail-cors", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Browser access:  enabled"));

    chatmail()
        .args(base.clone())
        .args(["webmail-cors", "disable"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Browser access disabled"));

    chatmail()
        .args(base)
        .args(["webmail-cors", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Browser access:  disabled"));
}

#[test]
fn e2e_language_set() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let base = state_argv(&state);

    chatmail()
        .args(base)
        .args(["language", "set", "es"])
        .assert()
        .success()
        .stdout(predicate::str::contains("es"));

    let db_path = effective_app_db_path(dir.path(), &AppConfig::default());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = init_db(&db_path).await.unwrap();
        assert_eq!(
            get_setting(&pool, settings_keys::LANGUAGE)
                .await
                .unwrap()
                .as_deref(),
            Some("es")
        );
    });
}

#[test]
fn e2e_html_export_writes_files() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let out = dir.path().join("www-out");
    let out_s = out.to_str().unwrap();

    chatmail()
        .args(state_argv(&state))
        .args(["html-export", out_s])
        .assert()
        .success()
        .stdout(predicate::str::contains("Successfully exported"));

    assert!(out.join("index.html").is_file());
}

/// Go-style custom www + legacy `/qr?data=` → `html-migrate --yes` rewrites templates and QR.
#[test]
fn e2e_html_migrate_go_templates_and_legacy_qr() {
    use std::fs;

    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().join("state");
    fs::create_dir_all(&state).unwrap();
    let www = dir.path().join("www");
    fs::create_dir_all(&www).unwrap();

    let conf = dir.path().join("chatmail.toml");
    fs::write(
        &conf,
        format!("hostname = \"e2e.test\"\nwww_dir = \"{}\"\n", www.display()),
    )
    .unwrap();

    fs::write(
        www.join("index.html"),
        r#"<!DOCTYPE html>
<html>
<head>
<script src="./main.js"></script>
</head>
<body>
{{if .RegistrationOpen}}open{{else}}closed{{end}}
<script>
document.getElementById("result-qr").src = `/qr?data=${encodeURIComponent(currentLink)}`;
</script>
</body>
</html>"#,
    )
    .unwrap();
    fs::write(www.join("main.js"), "function noop() {}\n").unwrap();

    let assert = chatmail()
        .args([
            "--state-dir",
            state.to_str().unwrap(),
            "--config",
            conf.to_str().unwrap(),
            "--json",
            "html-migrate",
            "--yes",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        stdout.contains("\"action\":\"migrated\"") || stdout.contains("\"action\": \"migrated\""),
        "stdout={stdout}"
    );
    assert!(
        stdout.contains("index.html"),
        "expected index.html in migrate JSON: {stdout}"
    );

    let body = fs::read_to_string(www.join("index.html")).unwrap();
    assert!(
        body.contains("{% if RegistrationOpen %}"),
        "Go→Minijinja: {body}"
    );
    assert!(
        body.contains("setQrCodeImage"),
        "legacy QR rewritten: {body}"
    );
    assert!(
        !body.contains("/qr?data="),
        "legacy /qr?data= should be gone: {body}"
    );
    assert!(body.contains("qrcode.min.js"), "qrcode script tag: {body}");
    assert!(
        www.join("qrcode.min.js").is_file(),
        "qrcode.min.js copied into www_dir"
    );
    let main = fs::read_to_string(www.join("main.js")).unwrap();
    assert!(
        main.contains("function setQrCodeImage"),
        "helper appended: {main}"
    );
    assert!(
        www.join("index.html.go-template.bak").is_file(),
        "HTML backup"
    );
    assert!(
        www.join("main.js.qr-compat.bak").is_file(),
        "main.js backup"
    );

    // Second run is a no-op (already migrated).
    chatmail()
        .args([
            "--state-dir",
            state.to_str().unwrap(),
            "--config",
            conf.to_str().unwrap(),
            "--json",
            "html-migrate",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("noop_already_migrated"));
}

/// Custom page with Obtainium-style `{%22` is warned about (not silently ignored).
#[test]
fn e2e_html_migrate_warns_on_literal_brace_url() {
    use std::fs;

    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().join("state");
    fs::create_dir_all(&state).unwrap();
    let www = dir.path().join("www");
    fs::create_dir_all(&www).unwrap();
    let conf = dir.path().join("chatmail.toml");
    fs::write(
        &conf,
        format!("hostname = \"e2e.test\"\nwww_dir = \"{}\"\n", www.display()),
    )
    .unwrap();

    // Minijinja-valid structure + Obtainium-style URL (will 500 if rendered without raw).
    fs::write(
        www.join("download.html"),
        r#"<!DOCTYPE html><a href="https://example.com/x?j={%22a%22:1}">x</a>"#,
    )
    .unwrap();

    let assert = chatmail()
        .args([
            "--state-dir",
            state.to_str().unwrap(),
            "--config",
            conf.to_str().unwrap(),
            "--json",
            "html-migrate",
            "--yes",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(stdout.contains("literal_brace_warnings"), "stdout={stdout}");
    assert!(
        stdout.contains("download.html") || stdout.contains("raw"),
        "expected warning payload: {stdout}"
    );
}

#[test]
fn e2e_openrelay_status_enable_disable_json() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let base = state_argv(&state);

    // Default status: denied, no DB override.
    let out = chatmail()
        .args(base.clone())
        .args(["openrelay", "status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&out).expect("openrelay status JSON");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "openrelay status");
    assert_eq!(envelope["data"]["allowed"], false);
    assert_eq!(envelope["data"]["file_default"], false);
    assert!(envelope["data"]["db_override"].is_null());
    assert_eq!(envelope["data"]["reload_required"], false);

    // Enable → DB true + JSON contract.
    let out = chatmail()
        .args(base.clone())
        .args(["openrelay", "enable", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&out).expect("openrelay enable JSON");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "openrelay enable");
    assert_eq!(envelope["data"]["allowed"], true);
    assert_eq!(envelope["data"]["reload_required"], true);
    assert!(
        envelope["message"]
            .as_str()
            .unwrap_or("")
            .to_ascii_lowercase()
            .contains("enabled"),
        "message={:?}",
        envelope["message"]
    );

    let out = chatmail()
        .args(base.clone())
        .args(["openrelay", "status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&out).expect("openrelay status after enable");
    assert_eq!(envelope["data"]["allowed"], true);
    assert_eq!(envelope["data"]["db_override"], true);

    // Disable → DB false.
    let out = chatmail()
        .args(base.clone())
        .args(["openrelay", "disable", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&out).expect("openrelay disable JSON");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["command"], "openrelay disable");
    assert_eq!(envelope["data"]["allowed"], false);
    assert_eq!(envelope["data"]["reload_required"], true);

    let out = chatmail()
        .args(base)
        .args(["openrelay", "status", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&out).expect("openrelay status after disable");
    assert_eq!(envelope["data"]["allowed"], false);
    assert_eq!(envelope["data"]["db_override"], false);

    let db_path = effective_app_db_path(dir.path(), &AppConfig::default());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = init_db(&db_path).await.unwrap();
        assert_eq!(
            get_setting(&pool, settings_keys::ALLOW_INBOUND_REMOTE_RCPT)
                .await
                .unwrap()
                .as_deref(),
            Some("false")
        );
        assert!(
            !get_bool_setting(&pool, settings_keys::ALLOW_INBOUND_REMOTE_RCPT, true)
                .await
                .unwrap()
        );
    });
}

#[test]
fn e2e_openrelay_human_status_and_enable() {
    let dir = TempDir::new().expect("tempdir");
    let state = dir.path().to_string_lossy().to_string();
    let base = state_argv(&state);

    chatmail()
        .args(base.clone())
        .args(["openrelay", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("denied"))
        .stdout(
            predicate::str::contains("file default").or(predicate::str::contains("File default")),
        );

    chatmail()
        .args(base.clone())
        .args(["openrelay", "enable"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ENABLED").or(predicate::str::contains("enabled")));

    chatmail()
        .args(base)
        .args(["openrelay", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("allowed"));
}
