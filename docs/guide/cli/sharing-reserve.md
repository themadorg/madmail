# `madmail sharing reserve`

Parent: [`sharing`](sharing.md)

Reserve a slug (link points to `reserved`)

## Synopsis

```bash
madmail sharing reserve [OPTIONS] <SLUG>
```

## JSON output (`--json`)

```bash
madmail sharing reserve --json
```

Success stdout:

```json
{"ok": true, "command": "sharing reserve", "data": { ... }}
```

Schema: [json-output.md](json-output.md#sharing-reserve).


---
[← `sharing`](sharing.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/sharing.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/sharing.rs)
