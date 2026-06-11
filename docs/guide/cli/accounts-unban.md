# `madmail accounts unban`

Parent: [`accounts`](accounts.md)

Remove blocklist entry only (does not restore mail/creds)

## Synopsis

```bash
madmail accounts unban [OPTIONS] <USERNAME>
```

## Options

| Option | Description |
|--------|-------------|
| `-y`, `--yes` | Skip confirmation prompt |

## Notes

Removes blocklist entry only — does **not** restore deleted mail or credentials.

## JSON output (`--json`)

```bash
madmail accounts unban --json
```

Success stdout:

```json
{"ok": true, "command": "accounts unban", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-unban).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
