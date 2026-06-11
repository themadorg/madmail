# `madmail federation flush`

Parent: [`federation`](federation.md)

Remove all domain exceptions

## Synopsis

```bash
madmail federation flush [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail federation flush --json
```

Success stdout:

```json
{"ok": true, "command": "federation flush", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation-flush).


---
[← `federation`](federation.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
