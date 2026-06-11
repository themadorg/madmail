# `madmail accounts delete`

Parent: [`accounts`](accounts.md)

Remove credentials, mail, and blocklist the address

## Synopsis

```bash
madmail accounts delete [OPTIONS] <USERNAME>
```

## Options

| Option | Description |
|--------|-------------|
| `-y`, `--yes` | Skip confirmation prompt |

## Notes

Permanently removes credentials, maildir, and adds blocklist entry with default reason.

## JSON output (`--json`)

```bash
madmail accounts delete --json
```

Success stdout:

```json
{"ok": true, "command": "accounts delete", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-delete).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
