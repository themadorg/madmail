use std::process::Command;

use assert_cmd::cargo::cargo_bin;
use tempfile::TempDir;

#[test]
fn test_binary_boots_and_migrates() {
    let state_dir = TempDir::new().expect("tempdir");
    let state_path = state_dir.path().to_string_lossy();
    // Isolate from repo `./data/chatmail.toml` (auto-detected when `--config` is omitted).
    let config_path = state_dir.path().join("boot-test.toml");
    let config_path = config_path.to_string_lossy();

    let output = Command::new(cargo_bin("chatmail"))
        .args([
            "--state-dir",
            &state_path,
            "--config",
            &config_path,
            "--boot-once",
        ])
        .output()
        .expect("run chatmail");

    assert!(
        output.status.success(),
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(state_dir.path().join("credentials.db").is_file());
    assert!(state_dir.path().join("admin_token").is_file());
}
