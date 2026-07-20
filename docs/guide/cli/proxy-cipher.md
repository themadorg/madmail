# `madmail proxy cipher`

Parent: [`proxy`](proxy.md)

View or change Shadowsocks cipher (`__SS_CIPHER__`).

## Synopsis

```bash
madmail proxy cipher [status|set|reset]
```

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show effective cipher and DB override (default) |
| `set <CIPHER>` | Set DB override |
| `reset` | Clear DB override (revert to config file) |

## Examples

```bash
madmail proxy cipher
madmail proxy cipher set aes-256-gcm
madmail proxy cipher reset
madmail reload
```

## Notes

Supported values: `aes-128-gcm`, `aes-256-gcm`, `chacha20-ietf-poly1305`. Requires `ss_addr` and `ss_password` in config.

## Subcommand pages

- [`status`](proxy-cipher-status.md) — `madmail proxy cipher status`
- [`set`](proxy-cipher-set.md) — `madmail proxy cipher set`
- [`reset`](proxy-cipher-reset.md) — `madmail proxy cipher reset`

## JSON output (`--json`)

```bash
madmail proxy cipher status --json
```

Success stdout:

```json
{"ok": true, "command": "proxy cipher status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#proxy-cipher-status).


---
[← `proxy`](proxy.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)