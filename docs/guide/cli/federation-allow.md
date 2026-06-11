# `madmail federation allow`

Parent: [`federation`](federation.md)

Add domain to rules (allowlist when policy is REJECT)

## Synopsis

```bash
madmail federation allow [OPTIONS] <DOMAIN>
```

## JSON output (`--json`)

```bash
madmail federation allow --json
```

Success stdout:

```json
{"ok": true, "command": "federation allow", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation-allow).


---
[← `federation`](federation.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
