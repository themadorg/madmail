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

//! Logging setup with Madmail No-Log defaults and maddy-compatible `log` targets.
//!
//! Config directive (static file only):
//! - `log off` / omit — No-Log (default)
//! - `log stderr` / `log on` / `log stderr_ts` — write to stderr
//! - `log syslog` — currently maps to stderr (no dedicated syslog backend yet)
//! - `log /path/to/file` — append to a file
//! - `log stderr /var/lib/madmail/madmail.log` — both
//!
//! `debug true` (flexible enable forms) overrides No-Log and forces `debug` filter level;
//! when no output target is set, debug logs go to stderr.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tracing_subscriber::{
    fmt::{self, format::FmtSpan, writer::BoxMakeWriter},
    prelude::*,
    reload::{self, Handle},
    EnvFilter, Registry,
};

pub type LogReloadHandle = Handle<EnvFilter, Registry>;

/// Parsed destinations from the `log` config directive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogDestinations {
    pub stderr: bool,
    pub files: Vec<PathBuf>,
}

impl LogDestinations {
    pub fn is_empty(&self) -> bool {
        !self.stderr && self.files.is_empty()
    }
}

/// Fan-out writer used by the fmt layer (stderr and/or open log files).
struct MultiLogWriter {
    stderr: bool,
    files: Vec<Arc<Mutex<File>>>,
}

impl Write for MultiLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.stderr {
            let _ = io::stderr().write_all(buf);
        }
        for f in &self.files {
            if let Ok(mut g) = f.lock() {
                let _ = g.write_all(buf);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.stderr {
            let _ = io::stderr().flush();
        }
        for f in &self.files {
            if let Ok(mut g) = f.lock() {
                let _ = g.flush();
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct SharedMultiLogWriter(Arc<Mutex<MultiLogWriter>>);

impl Write for SharedMultiLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).flush()
    }
}

/// True when config leaves logging disabled (`log off` or omitted — default is off).
pub fn maddy_log_off(log_target: Option<&str>) -> bool {
    !logging_enabled(log_target)
}

/// Whether the `log` directive enables tracing output (`stderr`, path, …).
pub fn logging_enabled(log_target: Option<&str>) -> bool {
    !parse_log_destinations(log_target).is_empty()
}

/// Whether tracing should be silenced (config `log` only; `debug true` in config overrides).
pub fn should_disable_logging(log_target: Option<&str>, debug: bool) -> bool {
    !debug && !logging_enabled(log_target)
}

/// Parse maddy-style `log` arguments (`stderr`, `off`, file paths, combinations).
pub fn parse_log_destinations(log_target: Option<&str>) -> LogDestinations {
    let Some(raw) = log_target.map(str::trim).filter(|s| !s.is_empty()) else {
        return LogDestinations::default();
    };

    let tokens: Vec<&str> = raw.split_whitespace().collect();
    if tokens.len() == 1 && tokens[0].eq_ignore_ascii_case("off") {
        return LogDestinations::default();
    }

    let mut dest = LogDestinations::default();
    for token in tokens {
        match token.to_ascii_lowercase().as_str() {
            "off" => {
                // Lone `off` handled above; ignore stray `off` among other targets.
            }
            "stderr" | "stderr_ts" | "on" | "syslog" => {
                // syslog → stderr until a real syslog backend is wired
                dest.stderr = true;
            }
            _ => dest.files.push(PathBuf::from(token)),
        }
    }
    dest
}

/// Destinations after applying debug override (debug + no targets → stderr).
pub fn effective_log_destinations(log_target: Option<&str>, debug: bool) -> LogDestinations {
    let mut dest = parse_log_destinations(log_target);
    if dest.is_empty() && debug {
        dest.stderr = true;
    }
    dest
}

/// Initialize a reloadable `tracing` subscriber. Returns a handle to toggle verbosity.
///
/// When `debug` is true in madmail.conf, tracing runs at `debug` regardless of a generic
/// `RUST_LOG=info` systemd drop-in (operators can still narrow via `RUST_LOG` when debug is off).
///
/// Output goes to the targets from `log` (stderr and/or files). No-Log uses filter `off`.
pub fn init_logging(debug: bool, log_target: Option<&str>) -> LogReloadHandle {
    let disabled = should_disable_logging(log_target, debug);
    let filter = if disabled {
        EnvFilter::new("off")
    } else if debug {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))
    };

    let (filter_layer, reload_handle) = reload::Layer::new(filter);
    let dest = effective_log_destinations(log_target, debug);
    let writer = make_log_writer(&dest);

    let subscriber = Registry::default().with(filter_layer).with(
        fmt::layer()
            .with_writer(writer)
            .with_span_events(FmtSpan::CLOSE)
            .with_ansi(false),
    );

    tracing::subscriber::set_global_default(subscriber)
        .expect("tracing subscriber must only be initialized once");

    reload_handle
}

fn open_log_file(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    OpenOptions::new().create(true).append(true).open(path)
}

fn make_log_writer(dest: &LogDestinations) -> BoxMakeWriter {
    if dest.is_empty() {
        return BoxMakeWriter::new(io::sink);
    }

    let mut files: Vec<Arc<Mutex<File>>> = Vec::new();
    for path in &dest.files {
        match open_log_file(path) {
            Ok(f) => files.push(Arc::new(Mutex::new(f))),
            Err(e) => {
                boot_error(format!(
                    "failed to open log file {}: {e} (continuing with remaining targets)",
                    path.display()
                ));
            }
        }
    }

    // Prefer configured stderr; if file open failed and nothing else works, fall back to stderr.
    let stderr = dest.stderr || files.is_empty();

    let shared = SharedMultiLogWriter(Arc::new(Mutex::new(MultiLogWriter { stderr, files })));
    BoxMakeWriter::new(move || shared.clone())
}

/// Fatal startup message on stderr (not affected by No-Log / tracing filter).
pub fn boot_error(message: impl std::fmt::Display) {
    eprintln!("chatmail: error: {message}");
}

/// Apply the No-Log policy by silencing all tracing output.
pub fn set_no_log(handle: &LogReloadHandle) {
    handle
        .modify(|filter| *filter = EnvFilter::new("off"))
        .expect("reload tracing filter");
}

/// Restore informational logging after No-Log was enabled.
#[allow(dead_code)]
pub fn set_info_log(handle: &LogReloadHandle) {
    handle
        .modify(|filter| *filter = EnvFilter::new("info"))
        .expect("reload tracing filter");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing::info;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn parse_log_off_and_default() {
        assert!(parse_log_destinations(None).is_empty());
        assert!(parse_log_destinations(Some("off")).is_empty());
        assert!(parse_log_destinations(Some("OFF")).is_empty());
        assert!(parse_log_destinations(Some("  ")).is_empty());
    }

    #[test]
    fn parse_log_stderr_variants() {
        for s in ["stderr", "STDERR", "on", "stderr_ts", "syslog"] {
            let d = parse_log_destinations(Some(s));
            assert!(d.stderr, "{s}");
            assert!(d.files.is_empty(), "{s}");
        }
    }

    #[test]
    fn parse_log_file_path() {
        let d = parse_log_destinations(Some("/var/lib/madmail/madmail.log"));
        assert!(!d.stderr);
        assert_eq!(d.files, vec![PathBuf::from("/var/lib/madmail/madmail.log")]);
    }

    #[test]
    fn parse_log_stderr_and_file() {
        let d = parse_log_destinations(Some("stderr /var/lib/madmail/madmail.log"));
        assert!(d.stderr);
        assert_eq!(d.files, vec![PathBuf::from("/var/lib/madmail/madmail.log")]);
    }

    #[test]
    fn debug_forces_stderr_when_no_log_target() {
        let d = effective_log_destinations(None, true);
        assert!(d.stderr);
        assert!(d.files.is_empty());
        let d = effective_log_destinations(Some("off"), true);
        assert!(d.stderr);
    }

    #[test]
    fn should_disable_respects_debug_and_log() {
        assert!(should_disable_logging(None, false));
        assert!(should_disable_logging(Some("off"), false));
        assert!(!should_disable_logging(Some("stderr"), false));
        assert!(!should_disable_logging(Some("/tmp/x.log"), false));
        assert!(!should_disable_logging(None, true));
        assert!(!should_disable_logging(Some("off"), true));
    }

    #[test]
    fn log_file_writer_appends() {
        use tracing_subscriber::fmt::MakeWriter;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.log");
        let dest = LogDestinations {
            stderr: false,
            files: vec![path.clone()],
        };
        let maker = make_log_writer(&dest);
        let mut w = maker.make_writer();
        w.write_all(b"hello-log-line\n").unwrap();
        w.flush().unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("hello-log-line"), "got: {content:?}");
    }

    /// P1-UT08: `ReloadHandle` drops events when filter is `off`.
    #[test]
    fn p1_ut08_dynamic_log_reload() {
        let events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let events_cap = Arc::clone(&events);

        let (filter_layer, reload_handle) = reload::Layer::new(EnvFilter::new("info"));
        let layer = fmt::layer().with_test_writer().with_writer({
            let events = events_cap;
            move || {
                let events = Arc::clone(&events);
                TestWriter(events)
            }
        });

        let subscriber = Registry::default().with(filter_layer).with(layer);
        tracing::subscriber::set_global_default(subscriber).unwrap();

        info!(target: "test", "visible");
        assert!(
            events.lock().unwrap().iter().any(|l| l.contains("visible")),
            "expected log line"
        );

        set_no_log(&reload_handle);
        events.lock().unwrap().clear();
        info!(target: "test", "hidden");
        assert!(events.lock().unwrap().is_empty(), "no output after No-Log");

        set_info_log(&reload_handle);
        events.lock().unwrap().clear();
        info!(target: "test", "visible again");
        assert!(!events.lock().unwrap().is_empty());
    }

    struct TestWriter(Arc<Mutex<Vec<String>>>);

    impl std::io::Write for TestWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Ok(s) = std::str::from_utf8(buf) {
                self.0.lock().unwrap().push(s.to_string());
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
