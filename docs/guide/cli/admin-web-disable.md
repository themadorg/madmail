# `madmail admin-web disable`

Parent: [`admin-web`](admin-web.md)

Disable the admin web dashboard

## Synopsis

```bash
madmail admin-web disable [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail admin web disable --json
```

Success stdout:

```json
{"ok": true, "command": "admin web disable", "data": { ... }}
```

Schema: [json-output.md](json-output.md#admin-web-disable).


---
[← `admin-web`](admin-web.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/admin_web.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/admin_web.rs)
