# `registration`

Open or close public account registration at `/new` (`__REGISTRATION_OPEN__`).


## Synopsis

```bash
madmail registration <open|close|status>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `open` | Allow registration when tokens/policy permit |
| `close` | Block new registrations |
| `status` | Show open/closed |

## Examples

```bash
madmail registration status
madmail registration open
madmail registration close
```

Combine with [registration-tokens](registration-tokens.md) for invite-only registration while closed.

## Subcommand pages

- [`close`](registration-close.md) — `madmail registration close`
- [`open`](registration-open.md) — `madmail registration open`
- [`status`](registration-status.md) — `madmail registration status`

## JSON output (`--json`)

```bash
madmail registration --json
```

Success stdout:

```json
{"ok": true, "command": "registration", "data": { ... }}
```

Schema: [json-output.md](json-output.md#registration).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/registration.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/registration.rs)
