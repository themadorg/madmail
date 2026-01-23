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
