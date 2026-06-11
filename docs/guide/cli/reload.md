# `reload`

Request a **soft reload** via the admin API: restarts listeners and HTTP routes in place without exiting the process.


## Synopsis

```bash
madmail reload [--url URL] [--insecure]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Options

| Flag | Description |
|------|-------------|
| `--url` | Override admin API base URL (default: from config + settings DB) |
| `--insecure` | Skip TLS verification (self-signed dev servers) |

## Example

```bash
madmail reload
madmail reload --insecure
```

## When to use

After changing DB-backed settings: ports, admin-web path, html-serve directory, push mode, etc.

Requires the server to be running and reachable at the admin API.

## JSON output (`--json`)

```bash
madmail reload --json
```

Success stdout:

```json
{"ok": true, "command": "reload", "data": { ... }}
```

Schema: [json-output.md](json-output.md#reload).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/reload.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/reload.rs)
