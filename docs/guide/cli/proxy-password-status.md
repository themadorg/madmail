# `madmail proxy password status`

Parent: [`proxy`](proxy.md) · [`password`](proxy-password.md)

Show password source (value hidden).

## Synopsis

```bash
madmail proxy password status [OPTIONS]
```

Omitting a subcommand under `password` is the same as `status`.

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
[← `password`](proxy-password.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)