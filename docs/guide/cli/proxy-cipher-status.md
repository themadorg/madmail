# `madmail proxy cipher status`

Parent: [`proxy`](proxy.md) · [`cipher`](proxy-cipher.md)

Show effective cipher and DB override.

## Synopsis

```bash
madmail proxy cipher status [OPTIONS]
```

Omitting a subcommand under `cipher` is the same as `status`.

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
[← `cipher`](proxy-cipher.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)