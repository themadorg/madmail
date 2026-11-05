# `madmail admin-web path`

Configuration precedence is **environment → explicit file setting → database → defaults**. A `CHATMAIL_*` variable, a `.conf` `settings { }` entry, or a top-level TOML setting protects its value from database-backed commands. Listener lines and other module directives are file defaults these commands override. File/environment changes require restart. See [configuration sources and migration](../configuration.md).

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
