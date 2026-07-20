# `madmail proxy cipher set`

Parent: [`proxy`](proxy.md) · [`cipher`](proxy-cipher.md)

Set DB cipher override.

## Synopsis

```bash
madmail proxy cipher set [OPTIONS] <CIPHER>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `CIPHER` | `aes-128-gcm`, `aes-256-gcm`, or `chacha20-ietf-poly1305` |

## Examples

```bash
madmail proxy cipher set aes-256-gcm
```

After changes, run `madmail reload` to apply.

## JSON output (`--json`)

```bash
madmail proxy cipher set --json
```

Success stdout:

```json
{"ok": true, "command": "proxy cipher set", "data": { ... }}
```

Schema: [json-output.md](json-output.md#proxy-cipher-setreset).


---
[← `cipher`](proxy-cipher.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)