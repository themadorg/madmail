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
use prometheus::{register_counter_vec, register_gauge_vec, Encoder, TextEncoder};

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
    // Create label children so `/metrics` is non-empty before the first SMTP event.
    let _ = STARTED.with_label_values(&["smtp"]);
    let _ = STARTED.with_label_values(&["submission"]);
    let _ = COMPLETED.with_label_values(&["smtp"]);
    let _ = COMPLETED.with_label_values(&["submission"]);
    let _ = ABORTED.with_label_values(&["smtp"]);
    let _ = ABORTED.with_label_values(&["submission"]);
    let _ = FAILED_LOGINS.with_label_values(&["smtp"]);
    let _ = FAILED_LOGINS.with_label_values(&["submission"]);
}

/// Full Prometheus text exposition (for tests and debugging).
pub fn exposition_text() -> Result<String, prometheus::Error> {
    init_metrics();
    let bytes = gather_bytes()?;
    Ok(String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned()))
}

pub(crate) fn gather_bytes() -> Result<Vec<u8>, prometheus::Error> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buf = Vec::new();
    encoder.encode(&metric_families, &mut buf)?;
    Ok(buf)
}

/// Parse a counter/gauge sample from Prometheus text exposition.
#[cfg(test)]
pub fn sample_value(body: &str, metric_name: &str, label_selector: &str) -> Option<f64> {
    for line in body.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if !line.starts_with(metric_name) {
            continue;
        }
        if !label_selector.is_empty() && !line.contains(label_selector) {
            continue;
        }
        let value = line.split_whitespace().last()?;
        return value.parse().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODULE: &str = "metrics_unit_test";

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
        let started =
            sample_value(&body, "maddy_smtp_started_transactions", &labels).unwrap();
        assert!(started >= 1.0, "started={started}");
        let queue =
            sample_value(&body, "maddy_queue_length", r#"location="/tmp/q""#).unwrap();
        assert!((queue - 2.0).abs() < f64::EPSILON, "queue={queue}");
    }
}
