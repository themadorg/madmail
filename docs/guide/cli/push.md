# `push`

Delta Chat push notifications via IMAP `XDELTAPUSH` (`__PUSH_MODE__`).


## Synopsis

```bash
madmail push <status|auto|on|off>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Mode, runtime status, failure counters |
| `auto` | Enabled until 5 consecutive notification-proxy failures (default) |
| `on` | Force push on |
| `off` | Force push off |

## Examples

```bash
madmail push status
madmail push auto
madmail push off
madmail reload
```

After changing settings stored in the database, run:

```bash
madmail reload
```

to apply listener and HTTP route changes without a full process restart.


Auto mode disables push after **5** consecutive failures (>20s timeout or HTTP error from the notification proxy).

## Subcommand pages

- [`auto`](push-auto.md) — `madmail push auto`
- [`off`](push-off.md) — `madmail push off`
- [`on`](push-on.md) — `madmail push on`
- [`status`](push-status.md) — `madmail push status`

## JSON output (`--json`)

```bash
madmail push --json
```

Success stdout:

```json
{"ok": true, "command": "push", "data": { ... }}
```

Schema: [json-output.md](json-output.md#push).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/push.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/push.rs)
