# `proxy`

Manage Shadowsocks circumvention proxy (`__SS_*__`). Alias: `pr`.


## Synopsis

```bash
madmail proxy [status|enable|disable|cipher|password]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show Shadowsocks configuration and client URL (default) |
| `enable` | Enable Shadowsocks listener |
| `disable` | Disable Shadowsocks listener |
| `cipher` | View or change cipher (`__SS_CIPHER__`) |
| `password` | View or change password (`__SS_PASSWORD__`) |

## Examples

```bash
madmail proxy status
madmail proxy enable
madmail proxy cipher set aes-256-gcm
madmail proxy password reset
madmail port shadowsocks set 8388
madmail reload
```

## Notes

Shadowsocks must be configured in the `chatmail { … }` block of `madmail.conf` with at least `ss_addr` and `ss_password`. Without those keys, `status` reports *not configured* and `enable` fails.

Supported ciphers: `aes-128-gcm`, `aes-256-gcm`, `chacha20-ietf-poly1305`.

`cipher` and `password` changes are stored as DB overrides. `reset` clears the override and reverts to the config file value.

Listener port and bind mode are managed with [`port shadowsocks`](port-shadowsocks.md), not `proxy`.

After any DB-backed change, run `madmail reload` to apply to a running server.

## Subcommand pages

- [`status`](proxy-status.md) — `madmail proxy status`
- [`enable`](proxy-enable.md) — `madmail proxy enable`
- [`disable`](proxy-disable.md) — `madmail proxy disable`
- [`cipher`](proxy-cipher.md) — `madmail proxy cipher …`
- [`password`](proxy-password.md) — `madmail proxy password …`

## JSON output (`--json`)

```bash
madmail proxy status --json
```

Success stdout:

```json
{"ok": true, "command": "proxy status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#proxy-status).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)