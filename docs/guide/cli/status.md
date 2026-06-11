# `status`

Show server health: active connections per service, user count, uptime, federation peers.


## Synopsis

```bash
madmail status [-d|--details]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Options

| Flag | Description |
|------|-------------|
| `-d`, `--details` | Per-port connection breakdown |

## Example

```bash
madmail status
madmail status --details
```

Reads listener ports from config and connection data from `{runtime_dir}/server_tracker.json` when the server is running.

## JSON output (`--json`)

```bash
madmail status --json
```

Success stdout:

```json
{"ok": true, "command": "status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#status).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/status_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/status_cmd.rs)
