# `madmail admin-web path`

Parent: [`admin-web`](admin-web.md)

Set or reset the admin web path

## Synopsis

```bash
madmail admin-web path [OPTIONS] [PATH]
```

## Options

| Option | Description |
|--------|-------------|
| `--reset` |  |
## Examples

```bash
madmail admin-web path /admin-secret
madmail admin-web path --reset
```

## Notes

Pass a path argument to set, or `--reset` to revert to `/admin`.

After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail admin web path --json
```

Success stdout:

```json
{"ok": true, "command": "admin web path", "data": { ... }}
```

Schema: [json-output.md](json-output.md#admin-web-path).


---
[← `admin-web`](admin-web.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/admin_web.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/admin_web.rs)
