# `madmail accounts delete-all`

Parent: [`accounts`](accounts.md)

Delete all user accounts (destructive)

## Synopsis

```bash
madmail accounts delete-all [OPTIONS]
```

## Options

| Option | Description |
|--------|-------------|
| `-y`, `--yes` | Skip confirmation prompt |

## Notes

Destructive: removes every user account. Requires `-y` / `--yes`.

## JSON output (`--json`)

```bash
madmail accounts delete all --json
```

Success stdout:

```json
{"ok": true, "command": "accounts delete all", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-delete-all).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
