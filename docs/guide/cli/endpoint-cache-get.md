# `madmail endpoint-cache get`

Parent: [`endpoint-cache`](endpoint-cache.md)

Show one entry

## Synopsis

```bash
madmail endpoint-cache get [OPTIONS] <LOOKUP_KEY>
```

## JSON output (`--json`)

```bash
madmail endpoint cache get --json
```

Success stdout:

```json
{"ok": true, "command": "endpoint cache get", "data": { ... }}
```

Schema: [json-output.md](json-output.md#endpoint-cache-get).


---
[← `endpoint-cache`](endpoint-cache.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/endpoint_cache.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/endpoint_cache.rs)
