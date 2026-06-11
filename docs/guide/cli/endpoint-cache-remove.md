# `madmail endpoint-cache remove`

Parent: [`endpoint-cache`](endpoint-cache.md)

Remove an entry

## Synopsis

```bash
madmail endpoint-cache remove [OPTIONS] <LOOKUP_KEY>
```


## Notes

Alias: `madmail endpoint-cache delete <LOOKUP_KEY>`.

## JSON output (`--json`)

```bash
madmail endpoint cache remove --json
```

Success stdout:

```json
{"ok": true, "command": "endpoint cache remove", "data": { ... }}
```

Schema: [json-output.md](json-output.md#endpoint-cache-remove).


---
[← `endpoint-cache`](endpoint-cache.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/endpoint_cache.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/endpoint_cache.rs)
