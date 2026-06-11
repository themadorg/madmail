# `madmail federation list`

Parent: [`federation`](federation.md)

Show current policy and all active rules

## Synopsis

```bash
madmail federation list [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail federation list --json
```

Success stdout:

```json
{"ok": true, "command": "federation list", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation-list).


---
[← `federation`](federation.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
