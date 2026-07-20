# `madmail proxy cipher reset`

Parent: [`proxy`](proxy.md) · [`cipher`](proxy-cipher.md)

Clear DB cipher override.

## Synopsis

```bash
madmail proxy cipher reset [OPTIONS]
```

## Notes

Reverts to `ss_cipher` from the config file (default `aes-128-gcm` when unset).

After changes, run `madmail reload` to apply.

## JSON output (`--json`)

```bash
madmail proxy cipher reset --json
```

Success stdout:

```json
{"ok": true, "command": "proxy cipher reset", "data": { ... }}
```

Schema: [json-output.md](json-output.md#proxy-cipher-setreset).


---
[← `cipher`](proxy-cipher.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)