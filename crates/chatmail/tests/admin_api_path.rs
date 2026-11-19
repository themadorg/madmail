//! `madmail admin-api path` against a state directory.
//!
//! Covers a stopped server and a local admin API that refuses a remount.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chatmail_db::{get_setting, init_db, set_setting, settings_keys};
use serde_json::Value;

fn run(state: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_madmail"))
        .arg("--state-dir")
        .arg(state)
        .arg("--config")
        .arg(state.join("missing.conf"))
        .args(args)
        .output()
        .expect("run madmail")
}

fn one_json(output: &std::process::Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let parsed: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|err| panic!("stdout is not one JSON object ({err}): {stdout:?}"));
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"], "admin-api path");
    parsed
}

#[tokio::test]
async fn offline_set_and_reset_json_persists_without_panic() {
    let dir = tempfile::tempdir().unwrap();
    init_db(&dir.path().join("credentials.db")).await.unwrap();
    std::fs::write(
        dir.path().join("admin_token"),
        format!("{}\n", "ab".repeat(32)),
    )
    .unwrap();

    let set = run(dir.path(), &["admin-api", "path", "/api/secret", "--json"]);
    let set = one_json(&set);
    assert_eq!(set["data"]["path"], "/api/secret");
    assert_eq!(set["data"]["applied"], false);
    assert_eq!(set["data"]["restart_required"], true);
    assert!(!set["message"]
        .as_str()
        .unwrap_or("")
        .to_ascii_lowercase()
        .contains("not running"));

    let pool = init_db(&dir.path().join("credentials.db")).await.unwrap();
    assert_eq!(
        get_setting(&pool, settings_keys::ADMIN_PATH)
            .await
            .unwrap()
            .as_deref(),
        Some("/api/secret")
    );
    drop(pool);

    let reset = run(dir.path(), &["admin-api", "path", "--reset", "--json"]);
    let reset = one_json(&reset);
    assert_eq!(reset["data"]["path"], "/api/admin");
    assert_eq!(reset["data"]["reset"], true);
    assert_eq!(reset["data"]["applied"], false);
    assert_eq!(reset["data"]["restart_required"], true);

    let pool = init_db(&dir.path().join("credentials.db")).await.unwrap();
    assert!(get_setting(&pool, settings_keys::ADMIN_PATH)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn reset_removes_a_path_that_is_not_the_mounted_route() {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_db(&dir.path().join("credentials.db")).await.unwrap();
    chatmail_db::set_setting(&pool, settings_keys::ADMIN_PATH, "/api/stranded")
        .await
        .unwrap();
    drop(pool);

    let reset = run(dir.path(), &["admin-api", "path", "--reset", "--json"]);
    let reset = one_json(&reset);
    assert_eq!(reset["data"]["path"], "/api/admin");
    assert_eq!(reset["data"]["applied"], false);

    let pool = init_db(&dir.path().join("credentials.db")).await.unwrap();
    assert!(get_setting(&pool, settings_keys::ADMIN_PATH)
        .await
        .unwrap()
        .is_none());
}

/// First candidate returns a reload error. The built-in URL is still tried.
#[tokio::test]
async fn reload_error_on_the_mounted_path_falls_back_and_keeps_the_save() {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_db(&dir.path().join("credentials.db")).await.unwrap();
    set_setting(&pool, settings_keys::ADMIN_PATH, "/api/old")
        .await
        .unwrap();
    drop(pool);
    std::fs::write(
        dir.path().join("admin_token"),
        format!("{}\n", "ab".repeat(32)),
    )
    .unwrap();

    let accepted = mock_admin(|path| path == "/api/admin");
    write_local_http_config(dir.path(), accepted.port);

    let set = run_configured(dir.path(), &["admin-api", "path", "/api/new", "--json"]);
    let set = one_json(&set);
    assert_eq!(set["data"]["path"], "/api/new");
    assert_eq!(set["data"]["applied"], true, "{set}");
    assert_eq!(set["data"]["restart_required"], false);
    assert!(set["data"].get("diagnostic").is_none(), "{set}");
    let hits = accepted.hits();
    assert_eq!(
        hits,
        vec!["/api/old".to_string(), "/api/admin".to_string()],
        "the built-in URL must be tried after the mounted path refuses"
    );

    let pool = init_db(&dir.path().join("credentials.db")).await.unwrap();
    assert_eq!(
        get_setting(&pool, settings_keys::ADMIN_PATH)
            .await
            .unwrap()
            .as_deref(),
        Some("/api/new")
    );
}

/// Every candidate refuses. The save still exits 0 with a restart envelope.
#[tokio::test]
async fn reload_errors_on_every_candidate_return_the_saved_path() {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_db(&dir.path().join("credentials.db")).await.unwrap();
    set_setting(&pool, settings_keys::ADMIN_PATH, "/api/old")
        .await
        .unwrap();
    drop(pool);
    std::fs::write(
        dir.path().join("admin_token"),
        format!("{}\n", "ab".repeat(32)),
    )
    .unwrap();

    let refused = mock_admin(|_| false);
    write_local_http_config(dir.path(), refused.port);

    let set = run_configured(dir.path(), &["admin-api", "path", "/api/new", "--json"]);
    assert!(
        set.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&set.stderr)
    );
    assert!(
        set.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&set.stderr)
    );
    let set = one_json(&set);
    assert_eq!(set["data"]["path"], "/api/new");
    assert_eq!(set["data"]["applied"], false, "{set}");
    assert_eq!(set["data"]["restart_required"], true);
    let diagnostic = set["data"]["diagnostic"].as_str().unwrap_or("");
    assert!(
        diagnostic.contains("reload failed"),
        "diagnostic: {diagnostic}"
    );
    assert!(diagnostic.contains("/api/old"), "{diagnostic}");
    assert!(diagnostic.contains("/api/admin"), "{diagnostic}");
    assert_eq!(
        refused.hits(),
        vec!["/api/old".to_string(), "/api/admin".to_string()]
    );

    let pool = init_db(&dir.path().join("credentials.db")).await.unwrap();
    assert_eq!(
        get_setting(&pool, settings_keys::ADMIN_PATH)
            .await
            .unwrap()
            .as_deref(),
        Some("/api/new")
    );
}

fn run_configured(state: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_madmail"))
        .arg("--state-dir")
        .arg(state)
        .arg("--config")
        .arg(state.join("madmail.conf"))
        .args(args)
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("all_proxy")
        .env("NO_PROXY", "*")
        .env("no_proxy", "*")
        .env_remove("RUST_LOG")
        .output()
        .expect("run madmail")
}

fn write_local_http_config(state: &std::path::Path, port: u16) {
    std::fs::write(
        state.join("madmail.conf"),
        format!("hostname 127.0.0.1\nhttp tcp://127.0.0.1:{port} {{\n}}\n"),
    )
    .unwrap();
}

struct MockAdmin {
    port: u16,
    hits: Arc<Mutex<Vec<String>>>,
    shutdown: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MockAdmin {
    fn hits(&self) -> Vec<String> {
        self.hits.lock().unwrap().clone()
    }
}

impl Drop for MockAdmin {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn mock_admin(accept: impl Fn(&str) -> bool + Send + 'static) -> MockAdmin {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let hits = Arc::new(Mutex::new(Vec::new()));
    let shutdown = Arc::new(AtomicBool::new(false));
    let recorded = Arc::clone(&hits);
    let stop = Arc::clone(&shutdown);
    let thread = thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let request = read_http_request(&mut stream);
                    let path = request_path(&request);
                    recorded.lock().unwrap().push(path.clone());
                    let body = if accept(&path) {
                        r#"{"status":200}"#
                    } else {
                        r#"{"status":500,"error":"reload failed"}"#
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => break,
            }
        }
    });
    MockAdmin {
        port,
        hits,
        shutdown,
        thread: Some(thread),
    }
}

fn read_http_request(stream: &mut impl Read) -> String {
    let mut buf = Vec::new();
    let mut tmp = [0_u8; 2048];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if let Some(header_end) = find_subsequence(&buf, b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&buf[..header_end]);
                    let length = content_length(&headers);
                    if buf.len() >= header_end + 4 + length {
                        break;
                    }
                }
                if buf.len() > 65_536 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn content_length(headers: &str) -> usize {
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn request_path(request: &str) -> String {
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("")
        .to_string()
}
