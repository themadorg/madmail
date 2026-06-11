# `madmail admin-web status`

Parent: [`admin-web`](admin-web.md)

Show admin web dashboard status

## Synopsis

```bash
madmail admin-web status [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail admin web status --json
```

Success stdout:

```json
{"ok": true, "command": "admin web status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#admin-web-status).


---
[← `admin-web`](admin-web.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/admin_web.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/admin_web.rs)
