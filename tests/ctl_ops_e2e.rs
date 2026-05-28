//! E2E: language, registration, webimap, html-export.

use std::process::Command;

use assert_cmd::cargo::cargo_bin;
use chatmail_config::effective_app_db_path;
use chatmail_config::AppConfig;
use chatmail_db::{get_bool_setting, get_setting, init_db, settings_keys};
use predicates::prelude::*;
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
            get_setting(&pool, settings_keys::LANGUAGE).await.unwrap().as_deref(),
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
