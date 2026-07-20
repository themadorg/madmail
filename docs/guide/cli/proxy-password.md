# `madmail proxy password`

Parent: [`proxy`](proxy.md)

View or change Shadowsocks password (`__SS_PASSWORD__`).

## Synopsis

```bash
madmail proxy password [status|set|reset]
```

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show password source (default; value hidden) |
| `set <PASSWORD>` | Set DB override |
| `reset` | Clear DB override (revert to config file) |

## Examples

```bash
madmail proxy password
madmail proxy password set 'new-secret'
madmail proxy password reset
madmail reload
```

## Notes

Password values are never printed in human or JSON output. Requires `ss_addr` and `ss_password` in config.

## Subcommand pages

- [`status`](proxy-password-status.md) — `madmail proxy password status`
- [`set`](proxy-password-set.md) — `madmail proxy password set`
- [`reset`](proxy-password-reset.md) — `madmail proxy password reset`

## JSON output (`--json`)

```bash
madmail proxy password status --json
```

Success stdout:

```json
{"ok": true, "command": "proxy password status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#proxy-password-status).


---
[← `proxy`](proxy.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)