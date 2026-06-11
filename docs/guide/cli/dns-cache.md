# `dns-cache`

Alias for [`endpoint-cache`](endpoint-cache.md).

```bash
madmail dns-cache   # same as: madmail endpoint-cache
```

All subcommands and flags are identical. See [endpoint-cache.md](endpoint-cache.md).

## JSON output (`--json`)

```bash
madmail dns cache --json
```

Success stdout:

```json
{"ok": true, "command": "dns cache", "data": { ... }}
```

Schema: [json-output.md](json-output.md#dns-cache).


---
[← CLI index](README.md)

[Source: `crates/chatmail/src/ctl/endpoint_cache.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/endpoint_cache.rs)
