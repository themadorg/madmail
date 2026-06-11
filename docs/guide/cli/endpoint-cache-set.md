# `madmail endpoint-cache set`

Parent: [`endpoint-cache`](endpoint-cache.md)

Create or update an entry (`LOOKUP_KEY TARGET_HOST [COMMENT]`)

## Synopsis

```bash
madmail endpoint-cache set [OPTIONS] <LOOKUP_KEY> <TARGET_HOST> [COMMENT]
```

## Examples

```bash
madmail endpoint-cache set a.com b.com "via partner"
```

## JSON output (`--json`)

```bash
madmail endpoint cache set --json
```

Success stdout:

```json
{"ok": true, "command": "endpoint cache set", "data": { ... }}
```

Schema: [json-output.md](json-output.md#endpoint-cache-set).


---
[← `endpoint-cache`](endpoint-cache.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/endpoint_cache.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/endpoint_cache.rs)
