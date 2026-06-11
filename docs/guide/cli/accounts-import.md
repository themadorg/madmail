# `madmail accounts import`

Parent: [`accounts`](accounts.md)

Import accounts from JSON file

## Synopsis

```bash
madmail accounts import [OPTIONS] <FILE>
```

## Examples

```bash
madmail accounts import backup.json
```

## Notes

JSON array of `{username, password?, hash?}` objects. Provide `password` or pre-computed `hash`.

## JSON output (`--json`)

```bash
madmail accounts import --json
```

Success stdout:

```json
{"ok": true, "command": "accounts import", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-import).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
