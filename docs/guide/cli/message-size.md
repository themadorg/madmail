# `message-size`

View or set maximum message size (`__APPENDLIMIT__` / `__MAX_MESSAGE_SIZE__`).


## Synopsis

```bash
madmail message-size [status|set|reset]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show effective limit and DB overrides (default) |
| `set <SIZE>` | Set both limits (e.g. `50M`, `1G`) |
| `reset` | Clear DB overrides |

## Examples

```bash
madmail message-size
madmail message-size set 100M
madmail message-size reset
```

## Subcommand pages

- [`reset`](message-size-reset.md) — `madmail message-size reset`
- [`set`](message-size-set.md) — `madmail message-size set`
- [`status`](message-size-status.md) — `madmail message-size status`

## JSON output (`--json`)

```bash
madmail message size --json
```

Success stdout:

```json
{"ok": true, "command": "message size", "data": { ... }}
```

Schema: [json-output.md](json-output.md#message-size).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/message_size.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/message_size.rs)
