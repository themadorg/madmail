//! End-to-end: the real `madmail monitor` binary scraping a live openmetrics listener.

use std::process::Command;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin;
use chatmail_metrics::run_openmetrics_listener;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn reserve_addr() -> String {
    let s = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    s.local_addr().expect("addr").to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn monitor_reads_a_live_openmetrics_listener() {
    let addr = reserve_addr();
    let cancel = CancellationToken::new();
    let listen = addr.clone();
    let cancel_bg = cancel.clone();
    let server = tokio::spawn(async move {
        run_openmetrics_listener(&listen, cancel_bg)
            .await
            .expect("openmetrics")
    });
    tokio::time::sleep(Duration::from_millis(80)).await;

    // An open connection in this process must show up in the separate
    // `madmail` process's scrape: gauge -> exporter -> HTTP -> CLI parser.
    let guard = chatmail_metrics::conn_guard("imap");

    // Point --config at a file that does not exist so the repo's
    // `./data/chatmail.toml` is not picked up (see boot_test.rs).
    let dir = TempDir::new().expect("tempdir");
    let config = dir.path().join("none.toml");
    let (config, state) = (
        config.to_string_lossy().into_owned(),
        dir.path().to_string_lossy().into_owned(),
    );
    let output = tokio::task::spawn_blocking(move || {
        Command::new(cargo_bin("madmail"))
            .args([
                "--json",
                "--config",
                &config,
                "--state-dir",
                &state,
                "monitor",
                "--addr",
                &addr,
                "--count",
                "1",
            ])
            .output()
            .expect("run madmail monitor")
    })
    .await
    .expect("join");
    drop(guard);

    assert!(
        output.status.success(),
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let line = stdout.lines().next().expect("one JSON line");
    let v: serde_json::Value = serde_json::from_str(line).expect("json envelope");

    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["command"], "monitor", "{v}");
    assert_eq!(v["data"]["conns"]["imap"], 1.0, "{v}");
    // Pre-created by init_metrics, so present and zero rather than absent.
    assert_eq!(v["data"]["conns"]["smtp"], 0.0, "{v}");
    assert_eq!(v["data"]["conns"]["submission"], 0.0, "{v}");
    // The first sample has nothing to measure a rate against.
    assert!(v["data"]["messages_per_second"].is_null(), "{v}");

    cancel.cancel();
    server.await.expect("server task");
}

#[test]
fn monitor_configuration_commands_are_opt_in() {
    let dir = TempDir::new().unwrap();
    for (name, contents) in [
        ("madmail.conf", "hostname mail.example.org\n"),
        ("chatmail.toml", "hostname = \"mail.example.org\"\n"),
    ] {
        let config = dir.path().join(name);
        std::fs::write(&config, contents).unwrap();
        for (action, enabled, changed) in [
            ("status", false, false),
            ("enable", true, true),
            ("enable", true, false),
            ("status", true, false),
            ("disable", false, true),
            ("disable", false, false),
        ] {
            let output = Command::new(cargo_bin("madmail"))
                .arg("--config")
                .arg(&config)
                .arg("--state-dir")
                .arg(dir.path())
                .args(["--json", "monitor", action])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["data"]["enabled"], enabled);
            assert_eq!(value["data"]["changed"], changed);
            assert_eq!(value["data"]["restart_required"], changed);
            assert_eq!(
                chatmail_config::load_config(&config)
                    .unwrap()
                    .openmetrics_listen
                    .is_some(),
                enabled
            );
        }
    }
}

#[test]
fn monitor_rejects_disabled_missing_and_invalid_configuration() {
    let dir = TempDir::new().unwrap();
    for (name, contents) in [
        ("disabled.conf", Some("hostname mail.example.org\n")),
        ("missing.conf", None),
        ("invalid.toml", Some("hostname = [\n")),
    ] {
        let config = dir.path().join(name);
        if let Some(contents) = contents {
            std::fs::write(&config, contents).unwrap();
        }
        let output = Command::new(cargo_bin("madmail"))
            .arg("--config")
            .arg(&config)
            .arg("--state-dir")
            .arg(dir.path())
            .args(["--json", "monitor", "--count", "1"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{name} unexpectedly succeeded");
        let value: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(value["ok"], false);
        if name != "invalid.toml" {
            assert!(String::from_utf8_lossy(&output.stderr).contains("madmail monitor enable"));
        }
        for action in ["enable", "disable"] {
            if name == "disabled.conf" {
                continue;
            }
            let before = std::fs::read(&config).ok();
            let output = Command::new(cargo_bin("madmail"))
                .arg("--config")
                .arg(&config)
                .arg("--state-dir")
                .arg(dir.path())
                .args(["monitor", action])
                .output()
                .unwrap();
            assert!(
                !output.status.success(),
                "{name}: {action} unexpectedly succeeded"
            );
            assert_eq!(std::fs::read(&config).ok(), before);
        }
    }
}

#[test]
fn monitor_reports_unreachable_endpoint() {
    let addr = reserve_addr();
    let dir = TempDir::new().unwrap();
    let output = Command::new(cargo_bin("madmail"))
        .arg("--config")
        .arg(dir.path().join("missing.conf"))
        .arg("--state-dir")
        .arg(dir.path())
        .args(["--json", "monitor", "--addr", &addr, "--count", "1"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["ok"], false);
    assert!(String::from_utf8_lossy(&output.stderr).contains("GET http://"));
}

#[tokio::test(flavor = "multi_thread")]
async fn monitor_uses_saved_credentials_and_rejects_wrong_password() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("madmail.conf");
    std::fs::write(&config, "hostname mail.example.org\n").unwrap();
    let listen = reserve_addr();
    let port = listen.rsplit_once(':').unwrap().1;
    let output = Command::new(cargo_bin("madmail"))
        .arg("--config")
        .arg(&config)
        .args([
            "--json",
            "monitor",
            "enable",
            "--ip",
            "127.0.0.1",
            "--port",
            port,
            "--username",
            "exporter",
            "--password",
            "integration-secret",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["listen"], listen);
    assert_eq!(value["data"]["authentication_required"], true);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("integration-secret"));
    let saved = chatmail_config::load_config(&config).unwrap();
    let cancel = CancellationToken::new();
    let child_cancel = cancel.clone();
    let server = tokio::spawn(async move {
        chatmail_metrics::run_openmetrics_listener_with_auth(
            saved.openmetrics_listen.as_deref().unwrap(),
            Some((
                saved.openmetrics_username.as_deref().unwrap(),
                saved.openmetrics_password.as_deref().unwrap(),
            )),
            child_cancel,
        )
        .await
        .unwrap();
    });
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(&listen).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for password in [None, Some("wrong-password")] {
        let mut command = Command::new(cargo_bin("madmail"));
        command
            .arg("--config")
            .arg(&config)
            .args(["--json", "monitor", "--count", "1"]);
        if let Some(password) = password {
            command.args(["--password", password]);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.success(), password.is_none());
        if password.is_some() {
            assert!(String::from_utf8_lossy(&output.stderr).contains("401"));
        }
        assert!(!String::from_utf8_lossy(&output.stdout).contains("integration-secret"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("integration-secret"));
    }
    for flags in [
        vec!["--ip", "invalid-ip"],
        vec!["--port", "0"],
        vec!["--port", "65536"],
    ] {
        let before = std::fs::read(&config).unwrap();
        let output = Command::new(cargo_bin("madmail"))
            .arg("--config")
            .arg(&config)
            .args(["monitor", "enable"])
            .args(flags)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(std::fs::read(&config).unwrap(), before);
    }
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}
