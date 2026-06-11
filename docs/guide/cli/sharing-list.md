# `madmail sharing list`

Parent: [`sharing`](sharing.md)

List all contact share links

## Synopsis

```bash
madmail sharing list [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail sharing list --json
```

Success stdout:

```json
{"ok": true, "command": "sharing list", "data": { ... }}
```

Schema: [json-output.md](json-output.md#sharing-list).


---
[← `sharing`](sharing.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/sharing.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/sharing.rs)
