# `madmail federation transport set`

Parent: [`federation transport`](federation-transport.md)

Set one outbound transport to `allow` or `block`. With `--domain`, `inherit` clears that destination's override for the named transport.

## Synopsis

```bash
madmail federation transport set <https|http|smtp> <allow|block> [--domain DOMAIN]
madmail federation transport set <https|http|smtp> inherit --domain DOMAIN
```

## Options

| Option | Description |
|--------|-------------|
| `--domain DOMAIN` | Destination domain. Omit it to change the global mode. `inherit` requires this flag. |

## Examples

```bash
madmail federation transport set http block
madmail federation transport set smtp block --domain evil.test
madmail federation transport set smtp inherit --domain evil.test
```

## JSON output (`--json`)

```bash
madmail federation transport set http block --json
```

Success stdout:

```json
{"ok": true, "command": "federation transport set", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation-transport-set).


---
[← `federation transport`](federation-transport.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
