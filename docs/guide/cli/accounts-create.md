# `madmail accounts create`

Parent: [`accounts`](accounts.md)

Create login + maildir + quota row

## Synopsis

```bash
madmail accounts create [OPTIONS] <USERNAME>
```

## Options

| Option | Description |
|--------|-------------|
| `-p`, `--password` | Password (prompted on stdin if omitted) |
## Examples

```bash
madmail accounts create alice@example.org --password 'secret'
```

## Notes

- `-p` / `--password`: omitted password is read from stdin (hidden prompt).
- Usernames without `@` are expanded using the registration domain from config.

## JSON output (`--json`)

```bash
madmail accounts create --json
```

Success stdout:

```json
{"ok": true, "command": "accounts create", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-create).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
