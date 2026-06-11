# `madmail port status`

Parent: [`port`](port.md)

Show mode and value for all admin-panel ports

## Synopsis

```bash
madmail port status [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail port status --json
```

Success stdout:

```json
{"ok": true, "command": "port status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-status).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
