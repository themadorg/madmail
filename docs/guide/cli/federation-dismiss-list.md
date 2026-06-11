# `madmail federation dismiss-list`

Parent: [`federation`](federation.md)

List all silent-dismiss domains

## Synopsis

```bash
madmail federation dismiss-list [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail federation dismiss list --json
```

Success stdout:

```json
{"ok": true, "command": "federation dismiss list", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation-dismiss-list).


---
[← `federation`](federation.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
