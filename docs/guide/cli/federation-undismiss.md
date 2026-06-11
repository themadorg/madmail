# `madmail federation undismiss`

Parent: [`federation`](federation.md)

Remove domain from silent-dismiss list

## Synopsis

```bash
madmail federation undismiss [OPTIONS] <DOMAIN>
```

## JSON output (`--json`)

```bash
madmail federation undismiss --json
```

Success stdout:

```json
{"ok": true, "command": "federation undismiss", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation-undismiss).


---
[← `federation`](federation.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
