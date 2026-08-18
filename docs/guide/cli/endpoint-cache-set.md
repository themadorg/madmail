# `madmail endpoint-cache set`

Parent: [`endpoint-cache`](endpoint-cache.md)

Create or update an entry (`LOOKUP_KEY TARGET_HOST [COMMENT]`)

## Synopsis

```bash
madmail endpoint-cache set [OPTIONS] <LOOKUP_KEY> <TARGET_HOST> [COMMENT]
```

## Examples

```bash
# Rewrite DNS name → another hostname (HTTPS then HTTP to /mxdeliv)
madmail endpoint-cache set a.com b.com "via partner"

# Route via madexchanger / local tunnel (prefer full URL)
madmail endpoint-cache set peer.example.org "http://127.0.0.1:19080/mxdeliv" "via madexchanger"

# IP-literal peers (register both bare and bracketed lookup keys if needed)
madmail endpoint-cache set 203.0.113.50 "http://127.0.0.1:19080/mxdeliv"
madmail endpoint-cache set "[203.0.113.50]" "http://127.0.0.1:19080/mxdeliv"
```

### `TARGET_HOST` forms

| Form | Behavior |
|------|----------|
| `hostname` or `1.2.3.4` | Deliver with `https://HOST/mxdeliv` then `http://HOST/mxdeliv` |
| `1.2.3.4:19080` | Same, with explicit port (IPv4+port must not be treated as IPv6) |
| `http://…` / `https://…` | Use as full rewrite URL (path defaults to `/mxdeliv` if omitted) |

For intermediate relays (madexchanger), **full URL** is the safest operator form.

See also: [federation IP / endpoint-port bugs](../../problems/federation-ip-normalize-and-endpoint-port.md).

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
