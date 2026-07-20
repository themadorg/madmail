# `madmail proxy password set`

Parent: [`proxy`](proxy.md) · [`password`](proxy-password.md)

Set DB password override.

## Synopsis

```bash
madmail proxy password set [OPTIONS] <PASSWORD>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `PASSWORD` | Non-empty Shadowsocks password |

## Examples

```bash
madmail proxy password set 'new-secret'
```

After changes, run `madmail reload` to apply.

## JSON output (`--json`)

```bash
madmail proxy password set --json
```

Success stdout:

```json
{"ok": true, "command": "proxy password set", "data": { ... }}
```

Schema: [json-output.md](json-output.md#proxy-password-setreset).


---
[← `password`](proxy-password.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)