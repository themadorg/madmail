# `madmail sharing edit`

Parent: [`sharing`](sharing.md)

Edit an existing share link (`SLUG NEW_URL [NEW_NAME]`)

## Synopsis

```bash
madmail sharing edit [OPTIONS] <SLUG> <NEW_URL> [NEW_NAME]
```

## JSON output (`--json`)

```bash
madmail sharing edit --json
```

Success stdout:

```json
{"ok": true, "command": "sharing edit", "data": { ... }}
```

Schema: [json-output.md](json-output.md#sharing-edit).


---
[← `sharing`](sharing.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/sharing.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/sharing.rs)
