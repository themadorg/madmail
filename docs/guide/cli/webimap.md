# `webimap`

Enable, disable, or inspect the WebIMAP HTTP API (`__WEBIMAP_ENABLED__`).


## Synopsis

```bash
madmail webimap <status|enable|disable>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show whether WebIMAP is enabled |
| `enable` | Enable the API |
| `disable` | Disable the API (HTTP 404) |

```bash
madmail webimap status
madmail webimap enable
```

These settings take effect on the next HTTP request. No reload or restart is
needed. See the [agent email quickstart](../../project/user-guide/19-ai-agent-email.md)
for an encrypted send/receive example using both APIs.

## Subcommand pages

- [`disable`](webimap-disable.md) — `madmail webimap disable`
- [`enable`](webimap-enable.md) — `madmail webimap enable`
- [`status`](webimap-status.md) — `madmail webimap status`

## JSON output (`--json`)

```bash
madmail webimap --json
```

Success stdout:

```json
{"ok": true, "command": "webimap", "data": { ... }}
```

Schema: [json-output.md](json-output.md#webimap).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/service_toggle.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/service_toggle.rs)
