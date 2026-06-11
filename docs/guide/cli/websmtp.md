# `websmtp`

Enable, disable, or inspect the WebSMTP HTTP send API (`__WEBSMTP_ENABLED__`).


## Synopsis

```bash
madmail websmtp <status|enable|disable>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show whether WebSMTP is enabled |
| `enable` | Enable the API |
| `disable` | Disable the API (HTTP 404) |

```bash
madmail websmtp disable
madmail reload
```

After changing settings stored in the database, run:

```bash
madmail reload
```

to apply listener and HTTP route changes without a full process restart.

## Subcommand pages

- [`disable`](websmtp-disable.md) — `madmail websmtp disable`
- [`enable`](websmtp-enable.md) — `madmail websmtp enable`
- [`status`](websmtp-status.md) — `madmail websmtp status`

## JSON output (`--json`)

```bash
madmail websmtp --json
```

Success stdout:

```json
{"ok": true, "command": "websmtp", "data": { ... }}
```

Schema: [json-output.md](json-output.md#websmtp).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/service_toggle.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/service_toggle.rs)
