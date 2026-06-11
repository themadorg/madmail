# `madmail endpoint-cache list`

Parent: [`endpoint-cache`](endpoint-cache.md)

List all endpoint override entries

## Synopsis

```bash
madmail endpoint-cache list [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail endpoint cache list --json
```

Success stdout:

```json
{"ok": true, "command": "endpoint cache list", "data": { ... }}
```

Schema: [json-output.md](json-output.md#endpoint-cache-list).


---
[← `endpoint-cache`](endpoint-cache.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/endpoint_cache.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/endpoint_cache.rs)
