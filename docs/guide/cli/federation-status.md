# `madmail federation status`

Parent: [`federation`](federation.md)

Show live federation traffic diagnostics from DB

## Synopsis

```bash
madmail federation status [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail federation status --json
```

Success stdout:

```json
{"ok": true, "command": "federation status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation-status).


---
[← `federation`](federation.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
