# `madmail admin-web enable`

Parent: [`admin-web`](admin-web.md)

Enable the admin web dashboard

## Synopsis

```bash
madmail admin-web enable [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail admin web enable --json
```

Success stdout:

```json
{"ok": true, "command": "admin web enable", "data": { ... }}
```

Schema: [json-output.md](json-output.md#admin-web-enable).


---
[← `admin-web`](admin-web.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/admin_web.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/admin_web.rs)
