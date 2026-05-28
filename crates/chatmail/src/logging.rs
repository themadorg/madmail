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

use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    prelude::*,
    reload::{self, Handle},
    EnvFilter, Registry,
};

pub type LogReloadHandle = Handle<EnvFilter, Registry>;

/// True when config leaves logging disabled (`log off` or omitted — default is off).
pub fn maddy_log_off(log_target: Option<&str>) -> bool {
    !logging_enabled(log_target)
}

/// Whether the `log` directive enables tracing output (`stderr`, `syslog`, …).
pub fn logging_enabled(log_target: Option<&str>) -> bool {
    match log_target {
        None => false,
        Some(t) if t.eq_ignore_ascii_case("off") => false,
        _ => true,
    }
}

/// Whether tracing should be silenced (config `log` only; `debug true` in config overrides).
pub fn should_disable_logging(log_target: Option<&str>, debug: bool) -> bool {
    !debug && !logging_enabled(log_target)
}

/// Initialize a reloadable `tracing` subscriber. Returns a handle to toggle verbosity.
pub fn init_logging(debug: bool) -> LogReloadHandle {
    let filter = if debug {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))
    };

    let (filter_layer, reload_handle) = reload::Layer::new(filter);

    let subscriber = Registry::default()
        .with(filter_layer)
        .with(fmt::layer().with_span_events(FmtSpan::CLOSE));

    tracing::subscriber::set_global_default(subscriber)
        .expect("tracing subscriber must only be initialized once");

    reload_handle
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
        assert!(
            events.lock().unwrap().is_empty(),
            "no output after No-Log"
        );

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
