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

//! `madmail monitor` — live connection and throughput metrics.
//!
//! Reads the `openmetrics` endpoint the server already exposes rather than
//! introducing a control socket, so the same numbers back the CLI, a Prometheus
//! scrape, and any future dashboard.

use std::time::{Duration, Instant};

use chatmail_config::{load_config, AppConfig, Args};
use chatmail_metrics::samples;
use chatmail_types::{ChatmailError, Result};

use super::output::CtlOut;

const CONNS_ACTIVE: &str = "maddy_conns_active";
const COMPLETED: &str = "maddy_smtp_smtp_completed_transactions";
const ABORTED: &str = "maddy_smtp_aborted_transactions";
const QUEUE_LENGTH: &str = "maddy_queue_length";

/// One scrape, reduced to the numbers the display needs.
#[derive(Debug, Default, PartialEq)]
struct Sample {
    imap: f64,
    smtp: f64,
    submission: f64,
    conns_total: f64,
    completed: f64,
    aborted: f64,
    queue: f64,
}

impl Sample {
    fn parse(body: &str) -> Self {
        let conns = samples(body, CONNS_ACTIVE);
        Self {
            imap: label_value(&conns, "imap"),
            smtp: label_value(&conns, "smtp"),
            submission: label_value(&conns, "submission"),
            conns_total: conns.iter().map(|(_, v)| v).sum(),
            completed: sum(body, COMPLETED),
            aborted: sum(body, ABORTED),
            queue: sum(body, QUEUE_LENGTH),
        }
    }
}

fn sum(body: &str, metric: &str) -> f64 {
    samples(body, metric).iter().map(|(_, v)| v).sum()
}

fn label_value(samples: &[(String, f64)], module: &str) -> f64 {
    let selector = format!(r#"module="{module}""#);
    samples
        .iter()
        .find(|(labels, _)| labels.contains(&selector))
        .map(|(_, v)| *v)
        .unwrap_or(0.0)
}

/// Per-second rate between two counter readings.
///
/// A server restart resets the counter, which would otherwise show up as a
/// large negative rate; Prometheus treats a counter going backwards as a reset
/// and reports 0 for that interval, and so do we.
fn rate(prev: f64, cur: f64, dt: Duration) -> f64 {
    let secs = dt.as_secs_f64();
    if secs <= 0.0 || cur < prev {
        return 0.0;
    }
    (cur - prev) / secs
}

/// `host:port` to scrape: `--addr`, else the configured `openmetrics` listener.
fn scrape_addr(config: &AppConfig, addr: Option<&str>) -> Result<String> {
    if let Some(addr) = addr {
        return Ok(dialable(addr));
    }
    match config.openmetrics_listen.as_deref() {
        Some(listen) => Ok(dialable(listen)),
        None => Err(ChatmailError::config(
            "openmetrics endpoint is not enabled; add `openmetrics tcp://127.0.0.1:9749 { }` \
             to the config (bind it to loopback - it has no authentication), \
             or pass --addr",
        )),
    }
}

/// A wildcard bind address is not a dial address - rewrite it to loopback.
fn dialable(listen: &str) -> String {
    match listen.rsplit_once(':') {
        Some(("0.0.0.0", port)) | Some(("", port)) => format!("127.0.0.1:{port}"),
        Some(("[::]", port)) | Some(("::", port)) => format!("[::1]:{port}"),
        _ => listen.to_string(),
    }
}

pub async fn monitor(
    args: &Args,
    interval: u64,
    count: Option<u64>,
    addr: Option<&str>,
) -> Result<()> {
    let out = CtlOut::from_args(args, "monitor");
    let config = if args.config.is_file() {
        load_config(&args.config)?
    } else {
        AppConfig::default()
    };
    let url = format!("http://{}/metrics", scrape_addr(&config, addr)?);
    let interval = Duration::from_secs(interval.max(1));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| ChatmailError::config(format!("monitor: HTTP client: {e}")))?;

    out.line(format!("Scraping {url} every {}s", interval.as_secs()));

    let mut previous: Option<(Sample, Instant)> = None;
    let mut printed = 0u64;
    loop {
        // The first row is printed immediately with no rate, so that
        // `--count 1` returns the current counts right away instead of
        // sleeping through an interval first.
        if previous.is_some() {
            tokio::time::sleep(interval).await;
        }

        let body = scrape(&client, &url).await?;
        let now = Instant::now();
        let sample = Sample::parse(&body);

        // Header only once the first scrape has actually worked, so a failure
        // to reach the endpoint is not preceded by an empty table.
        if previous.is_none() {
            out.line(format!(
                "{:>6} {:>6} {:>10} {:>7} {:>9} {:>9} {:>7}",
                "imap", "smtp", "submission", "total", "msg/s", "aborted/s", "queue"
            ));
        }

        let rates = previous.as_ref().map(|(prev, at)| {
            let dt = now.duration_since(*at);
            (
                rate(prev.completed, sample.completed, dt),
                rate(prev.aborted, sample.aborted, dt),
            )
        });

        report(&out, &sample, rates)?;
        previous = Some((sample, now));

        printed += 1;
        if count.is_some_and(|n| printed >= n) {
            return Ok(());
        }
    }
}

async fn scrape(client: &reqwest::Client, url: &str) -> Result<String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| ChatmailError::config(format!("monitor: GET {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(ChatmailError::config(format!(
            "monitor: GET {url}: HTTP {}",
            resp.status()
        )));
    }
    resp.text()
        .await
        .map_err(|e| ChatmailError::config(format!("monitor: reading {url}: {e}")))
}

fn report(out: &CtlOut, s: &Sample, rates: Option<(f64, f64)>) -> Result<()> {
    if out.is_json() {
        // One envelope per sample, so `--json` streams as NDJSON.
        return out.emit(serde_json::json!({
            "conns": {
                "imap": s.imap,
                "smtp": s.smtp,
                "submission": s.submission,
                "total": s.conns_total,
            },
            "messages_per_second": rates.map(|(msg, _)| msg),
            "aborted_per_second": rates.map(|(_, aborted)| aborted),
            "completed_total": s.completed,
            "aborted_total": s.aborted,
            "queue_length": s.queue,
        }));
    }

    let (msg, aborted) = match rates {
        Some((msg, aborted)) => (format!("{msg:.2}"), format!("{aborted:.2}")),
        // No previous sample yet, so there is no interval to average over.
        None => ("-".to_string(), "-".to_string()),
    };
    out.line(format!(
        "{:>6} {:>6} {:>10} {:>7} {:>9} {:>9} {:>7}",
        s.imap as i64,
        s.smtp as i64,
        s.submission as i64,
        s.conns_total as i64,
        msg,
        aborted,
        s.queue as i64,
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "\
# TYPE maddy_conns_active gauge
maddy_conns_active{module=\"imap\"} 4
maddy_conns_active{module=\"smtp\"} 2
maddy_conns_active{module=\"submission\"} 1
maddy_smtp_smtp_completed_transactions{module=\"smtp\"} 10
maddy_smtp_smtp_completed_transactions{module=\"submission\"} 5
maddy_smtp_aborted_transactions{module=\"smtp\"} 3
maddy_queue_length{module=\"remote_queue\",location=\"/tmp/q\"} 7
";

    #[test]
    fn parses_a_scrape_into_totals() {
        let s = Sample::parse(BODY);
        assert_eq!(s.imap, 4.0);
        assert_eq!(s.smtp, 2.0);
        assert_eq!(s.submission, 1.0);
        assert_eq!(s.conns_total, 7.0);
        // Summed across module labels.
        assert_eq!(s.completed, 15.0);
        assert_eq!(s.aborted, 3.0);
        assert_eq!(s.queue, 7.0);
    }

    #[test]
    fn missing_metrics_read_as_zero() {
        assert_eq!(Sample::parse(""), Sample::default());
    }

    #[test]
    fn rate_is_per_second_and_survives_a_counter_reset() {
        assert_eq!(rate(10.0, 20.0, Duration::from_secs(2)), 5.0);
        // Server restarted between scrapes: report 0, not a huge negative.
        assert_eq!(rate(100.0, 1.0, Duration::from_secs(2)), 0.0);
        // Two scrapes in the same instant would divide by zero.
        assert_eq!(rate(1.0, 9.0, Duration::ZERO), 0.0);
    }

    #[test]
    fn wildcard_bind_addresses_are_dialled_on_loopback() {
        assert_eq!(dialable("0.0.0.0:9749"), "127.0.0.1:9749");
        assert_eq!(dialable("[::]:9749"), "[::1]:9749");
        assert_eq!(dialable("127.0.0.1:9749"), "127.0.0.1:9749");
        assert_eq!(dialable("metrics.internal:9749"), "metrics.internal:9749");
    }

    #[test]
    fn scrape_addr_prefers_the_flag_then_the_config() {
        let mut config = AppConfig::default();
        assert_eq!(
            scrape_addr(&config, Some("10.0.0.1:1234")).unwrap(),
            "10.0.0.1:1234"
        );

        // Neither set: the error has to say how to turn the endpoint on.
        let err = scrape_addr(&config, None).unwrap_err().to_string();
        assert!(err.contains("openmetrics"), "unhelpful error: {err}");

        config.openmetrics_listen = Some("0.0.0.0:9749".to_string());
        assert_eq!(scrape_addr(&config, None).unwrap(), "127.0.0.1:9749");
    }
}
