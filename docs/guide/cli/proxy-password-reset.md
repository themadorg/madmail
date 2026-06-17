# `madmail proxy password reset`

Parent: [`proxy`](proxy.md) · [`password`](proxy-password.md)

Clear DB password override.

## Synopsis

```bash
madmail proxy password reset [OPTIONS]
```

## Notes

Reverts to `ss_password` from the config file.

After changes, run `madmail reload` to apply.

## JSON output (`--json`)

```bash
madmail proxy password reset --json
```

Success stdout:

```json
{"ok": true, "command": "proxy password reset", "data": { ... }}
```

Schema: [json-output.md](json-output.md#proxy-password-setreset).


---
[← `password`](proxy-password.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)