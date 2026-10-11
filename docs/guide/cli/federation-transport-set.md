# `madmail federation transport set`

Parent: [`federation transport`](federation-transport.md)

Set one outbound transport to `allow` or `block`.

## Synopsis

```bash
madmail federation transport set <https|http|smtp> <allow|block>
```

## Examples

```bash
madmail federation transport set http block
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
