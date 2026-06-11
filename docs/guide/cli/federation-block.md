# `madmail federation block`

Parent: [`federation`](federation.md)

Add domain to rules (blocklist when policy is ACCEPT)

## Synopsis

```bash
madmail federation block [OPTIONS] <DOMAIN>
```

## Examples

```bash
madmail federation block evil.net
```

## JSON output (`--json`)

```bash
madmail federation block --json
```

Success stdout:

```json
{"ok": true, "command": "federation block", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation-block).


---
[← `federation`](federation.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
