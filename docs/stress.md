# Stress Testing Madmail

This document explains how to run the Delta Chat stress test and how to
interpret the report output.

## Prerequisites

- A reachable Madmail instance (single server).
- `deltachat-rpc-server` installed locally at `/usr/bin/deltachat-rpc-server`.
- Python deps installed via `uv sync --frozen` (from repo root).

## Run the stress test

Set the target server and run the stress test:

```bash
REMOTE1=100.64.0.21 \
uv run python tests/deltachat-test/main.py \
  --stress \
  --stress-users 100 \
  --stress-workers 8 \
  --stress-duration 10
```

Parameters:
- `--stress-users`: total users to create.
- `--stress-workers`: number of worker processes.
- `--stress-duration`: send window in seconds.

## Go Stressor (recommended for protocol load)

For repeatable high-concurrency SMTP load without Python process overhead, use
the native Go stress tool:

```bash
go run ./cmd/madmail-stressor \
  -target 127.0.0.1:25 \
  -mail-from loadtest@example.net \
  -rcpt-to sink@example.net \
  -ramp 32,64,128,256 \
  -duration 30s \
  -body-bytes 256 \
  -report-json tmp/stressor-report.json \
  -report-md tmp/stressor-report.md
```

Useful flags:
- `-ramp`: sequential worker stages for capacity discovery.
- `-duration`: run length for each stage.
- `-messages-per-worker`: fixed attempts per worker per stage (deterministic runs).
- `-connect-timeout` and `-io-timeout`: fail fast under overload.
- `-max-latency-samples`: bound in-memory latency samples.

The tool reports:
- attempts/successes/failures
- success rate
- throughput (messages/sec)
- latency (avg/p50/p95/p99/min/max)
- aggregated error classes (e.g. timeout, smtp_4xx, smtp_5xx)

Results are written under `tmp/test_run_YYYYMMDD_HHMMSS/`:
- `stress_report.json`
- `stress_report.md` (stakeholder-friendly)

## Interpreting the report

Key fields:
- `messages_sent`: total number of send attempts during the window.
- `send_rate_mps`: aggregate send rate (messages/sec).
- `avg per-user send rate`: send rate divided by users.

Important notes:
- The send rate is based on client-side send attempts, not confirmed delivery.
- The report does not capture server CPU/RAM usage unless you measure it
  separately.

## Example interpretation

Example results:
- Users: 100
- Workers: 8
- Duration: 10s
- Send rate: 2094.28 messages/sec

Interpretation:
- The system sustained ~2.1k send attempts/sec across 100 users.
- Average per-user send rate was ~20.9 messages/sec.
- Secure Join succeeded, so encrypted messaging setup was valid.

## Troubleshooting

If the run hangs:
- Check `tmp/test_run_*/client_debug_worker_*.log` for errors.
- Ensure `deltachat-rpc-server` is installed and executable.
