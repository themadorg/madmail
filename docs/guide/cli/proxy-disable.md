# `madmail proxy disable`

Parent: [`proxy`](proxy.md)

Disable Shadowsocks listener.

## Synopsis

```bash
madmail proxy disable [OPTIONS]
```

## Notes

Sets `__SS_ENABLED__` to false in the database. The listener stops after `madmail reload`.

## JSON output (`--json`)

```bash
madmail proxy disable --json
```

Success stdout:

```json
{"ok": true, "command": "proxy disable", "data": { ... }}
```

Schema: [json-output.md](json-output.md#proxy-enabledisable).


---
[← `proxy`](proxy.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)