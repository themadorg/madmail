# `madmail federation dismiss-flush`

Parent: [`federation`](federation.md)

Remove all silent-dismiss domains

## Synopsis

```bash
madmail federation dismiss-flush [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail federation dismiss flush --json
```

Success stdout:

```json
{"ok": true, "command": "federation dismiss flush", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation-dismiss-flush).


---
[← `federation`](federation.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
