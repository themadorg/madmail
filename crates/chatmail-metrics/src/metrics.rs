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

use once_cell::sync::Lazy;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_int_gauge_vec, Encoder, TextEncoder,
};

static STARTED: Lazy<prometheus::CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "maddy_smtp_started_transactions",
        "Amount of SMTP transactions started",
        &["module"]
    )
    .unwrap()
});

static COMPLETED: Lazy<prometheus::CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "maddy_smtp_smtp_completed_transactions",
        "Amount of SMTP transactions successfully completed",
        &["module"]
    )
    .unwrap()
});

static ABORTED: Lazy<prometheus::CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "maddy_smtp_aborted_transactions",
        "Amount of SMTP transactions aborted",
        &["module"]
    )
    .unwrap()
});

#[allow(dead_code)]
static RATELIMIT_DEFERRED: Lazy<prometheus::CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "maddy_smtp_ratelimit_deferred",
        "Messages rejected with 4xx code due to ratelimiting",
        &["module"]
    )
    .unwrap()
});

static FAILED_LOGINS: Lazy<prometheus::CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "maddy_smtp_failed_logins",
        "AUTH command failures",
        &["module"]
    )
    .unwrap()
});

static FAILED_COMMANDS: Lazy<prometheus::CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "maddy_smtp_failed_commands",
        "Failed transaction commands (MAIL, RCPT, DATA)",
        &["module", "command", "smtp_code", "smtp_enchcode"]
    )
    .unwrap()
});

static CONNS_ACTIVE: Lazy<prometheus::IntGaugeVec> = Lazy::new(|| {
    register_int_gauge_vec!(
        "maddy_conns_active",
        "Amount of client connections currently open",
        &["module"]
    )
    .unwrap()
});

static QUEUE_LENGTH: Lazy<prometheus::GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "maddy_queue_length",
        "Amount of queued messages",
        &["module", "location"]
    )
    .unwrap()
});

pub fn record_smtp_started(module: &str) {
    STARTED.with_label_values(&[module]).inc();
}

pub fn record_smtp_completed(module: &str) {
    COMPLETED.with_label_values(&[module]).inc();
}

pub fn record_smtp_aborted(module: &str) {
    ABORTED.with_label_values(&[module]).inc();
}

pub fn record_smtp_failed_login(module: &str) {
    FAILED_LOGINS.with_label_values(&[module]).inc();
}

pub fn record_smtp_failed_command(module: &str, command: &str, smtp_code: u16, enchcode: &str) {
    FAILED_COMMANDS
        .with_label_values(&[module, command, &smtp_code.to_string(), enchcode])
        .inc();
}

#[allow(dead_code)]
pub fn record_smtp_ratelimit_deferred(module: &str) {
    RATELIMIT_DEFERRED.with_label_values(&[module]).inc();
}

/// Count one open connection for `module` until the returned guard is dropped.
///
/// The count is released on drop rather than by an explicit close call so that
/// it survives every way a connection task can end - a failed TLS handshake, an
/// early return, a panic, or the runtime dropping the task at shutdown. Bind it
/// (`let _guard = ...`), never `let _ = ...`, which drops it immediately.
#[must_use = "the connection is counted only while the guard is alive"]
pub fn conn_guard(module: &str) -> ConnGuard {
    let gauge = CONNS_ACTIVE.with_label_values(&[module]);
    gauge.inc();
    ConnGuard { gauge }
}

/// Decrements `maddy_conns_active` for its module when dropped.
pub struct ConnGuard {
    gauge: prometheus::IntGauge,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.gauge.dec();
    }
}

pub fn set_queue_length(module: &str, location: &str, depth: f64) {
    QUEUE_LENGTH
        .with_label_values(&[module, location])
        .set(depth);
}

/// Register all metric families with the global registry (call before serving `/metrics`).
pub fn init_metrics() {
    let _ = &*STARTED;
    let _ = &*COMPLETED;
    let _ = &*ABORTED;
    let _ = &*RATELIMIT_DEFERRED;
    let _ = &*FAILED_LOGINS;
    let _ = &*FAILED_COMMANDS;
    let _ = &*QUEUE_LENGTH;
    let _ = &*CONNS_ACTIVE;
    // Create label children so `/metrics` is non-empty before the first SMTP event.
    let _ = STARTED.with_label_values(&["smtp"]);
    let _ = STARTED.with_label_values(&["submission"]);
    let _ = COMPLETED.with_label_values(&["smtp"]);
    let _ = COMPLETED.with_label_values(&["submission"]);
    let _ = ABORTED.with_label_values(&["smtp"]);
    let _ = ABORTED.with_label_values(&["submission"]);
    let _ = FAILED_LOGINS.with_label_values(&["smtp"]);
    let _ = FAILED_LOGINS.with_label_values(&["submission"]);
    let _ = CONNS_ACTIVE.with_label_values(&["smtp"]);
    let _ = CONNS_ACTIVE.with_label_values(&["submission"]);
    let _ = CONNS_ACTIVE.with_label_values(&["imap"]);
}

/// Full Prometheus text exposition (for tests and debugging).
pub fn exposition_text() -> Result<String, prometheus::Error> {
    init_metrics();
    let bytes = gather_bytes()?;
    Ok(String::from_utf8(bytes)
        .unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned()))
}

pub(crate) fn gather_bytes() -> Result<Vec<u8>, prometheus::Error> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buf = Vec::new();
    encoder.encode(&metric_families, &mut buf)?;
    Ok(buf)
}

/// Parse a counter/gauge sample from Prometheus text exposition.
/// Every sample of `metric_name` in a Prometheus text exposition, as
/// `(label set, value)` - the label set is the raw text between the braces, or
/// empty for an unlabelled metric.
///
/// The name must match exactly: asking for `maddy_smtp_started` does not return
/// samples of `maddy_smtp_started_transactions`.
pub fn samples(body: &str, metric_name: &str) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(rest) = line.strip_prefix(metric_name) else {
            continue;
        };
        let (labels, rest) = match rest.strip_prefix('{') {
            Some(after) => match after.split_once('}') {
                Some((labels, rest)) => (labels.to_string(), rest),
                None => continue,
            },
            // Without braces the name must end here, otherwise this is a
            // different metric that merely starts with the same characters.
            None if rest.starts_with(char::is_whitespace) => (String::new(), rest),
            None => continue,
        };
        // The exposition format is `name{labels} value [timestamp]`; this
        // encoder never emits timestamps.
        if let Some(value) = rest.split_whitespace().next().and_then(|v| v.parse().ok()) {
            out.push((labels, value));
        }
    }
    out
}

/// First sample of `metric_name` whose label set contains `label_selector`
/// (empty selector matches the first sample of any label set).
pub fn sample_value(body: &str, metric_name: &str, label_selector: &str) -> Option<f64> {
    samples(body, metric_name)
        .into_iter()
        .find(|(labels, _)| label_selector.is_empty() || labels.contains(label_selector))
        .map(|(_, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODULE: &str = "metrics_unit_test";

    #[test]
    fn samples_matches_the_exact_metric_name() {
        let body = "\
# HELP maddy_smtp_started_transactions Amount of SMTP transactions started
# TYPE maddy_smtp_started_transactions counter
maddy_smtp_started_transactions{module=\"smtp\"} 7
maddy_smtp_started_transactions{module=\"submission\"} 3
maddy_queue_length{module=\"remote_queue\",location=\"/tmp/q\"} 2
bare_metric 5
";

        let started = samples(body, "maddy_smtp_started_transactions");
        assert_eq!(started.len(), 2);
        assert_eq!(started[0].1, 7.0);
        assert_eq!(started[1].1, 3.0);
        assert!(started[0].0.contains(r#"module="smtp""#));

        // A shorter name that is a prefix of a real one must not match it.
        assert!(samples(body, "maddy_smtp_started").is_empty());

        assert_eq!(samples(body, "bare_metric"), vec![(String::new(), 5.0)]);

        assert_eq!(
            sample_value(
                body,
                "maddy_smtp_started_transactions",
                r#"module="submission""#
            ),
            Some(3.0)
        );
        assert_eq!(sample_value(body, "maddy_no_such_metric", ""), None);
    }

    #[test]
    fn conn_guard_counts_while_alive() {
        // Unique label: the gauge is a global static and tests share the process.
        let module = "conn_guard_alive_test";
        let gauge = CONNS_ACTIVE.with_label_values(&[module]);

        let guards: Vec<_> = (0..3).map(|_| conn_guard(module)).collect();
        assert_eq!(gauge.get(), 3);

        drop(guards);
        assert_eq!(gauge.get(), 0);
    }

    #[test]
    fn conn_guard_releases_on_panic() {
        let module = "conn_guard_panic_test";
        let gauge = CONNS_ACTIVE.with_label_values(&[module]);

        let result = std::panic::catch_unwind(|| {
            let _guard = conn_guard(module);
            panic!("session died mid-connection");
        });
        assert!(result.is_err());

        // A connection task that panics must not leak a count, otherwise the
        // gauge only ever climbs and the whole metric becomes useless.
        assert_eq!(gauge.get(), 0);
    }

    #[test]
    fn gather_after_init_is_non_empty() {
        init_metrics();
        let body = gather_bytes().expect("encode");
        assert!(!body.is_empty(), "expected HELP/TYPE lines after init");
    }

    #[test]
    fn exposition_text_is_non_empty() {
        let text = exposition_text().expect("exposition");
        assert!(
            text.contains("maddy_smtp_started_transactions"),
            "text: {text}"
        );
    }

    #[test]
    fn gather_exposes_maddy_smtp_and_queue_metrics() {
        init_metrics();
        record_smtp_started(MODULE);
        record_smtp_completed(MODULE);
        record_smtp_failed_login(MODULE);
        record_smtp_failed_command(MODULE, "MAIL", 501, "5.5.4");
        set_queue_length("remote_queue", "/tmp/q", 2.0);

        let body = String::from_utf8(gather_bytes().expect("encode")).expect("utf8");
        assert!(
            body.contains("maddy_smtp_started_transactions"),
            "missing started counter: {body}"
        );
        assert!(
            body.contains(&format!(r#"module="{MODULE}""#)),
            "missing module label: {body}"
        );
        assert!(body.contains("maddy_smtp_smtp_completed_transactions"));
        assert!(body.contains("maddy_smtp_failed_logins"));
        assert!(body.contains("maddy_smtp_failed_commands"));
        assert!(body.contains("maddy_queue_length"));

        let labels = format!(r#"module="{MODULE}""#);
        let started = sample_value(&body, "maddy_smtp_started_transactions", &labels).unwrap();
        assert!(started >= 1.0, "started={started}");
        let queue = sample_value(&body, "maddy_queue_length", r#"location="/tmp/q""#).unwrap();
        assert!((queue - 2.0).abs() < f64::EPSILON, "queue={queue}");
    }
}
