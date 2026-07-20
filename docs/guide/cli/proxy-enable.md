# `madmail proxy enable`

Parent: [`proxy`](proxy.md)

Enable Shadowsocks listener.

## Synopsis

```bash
madmail proxy enable [OPTIONS]
```

## Notes

Requires `ss_addr` and `ss_password` in the configuration file. Sets `__SS_ENABLED__` in the database.

After changes, run `madmail reload` to apply.

## JSON output (`--json`)

```bash
madmail proxy enable --json
```

Success stdout:

```json
{"ok": true, "command": "proxy enable", "data": { ... }}
```

Schema: [json-output.md](json-output.md#proxy-enabledisable).


---
[← `proxy`](proxy.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)