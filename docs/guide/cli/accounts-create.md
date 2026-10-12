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
- With a primary-domain list, usernames without `@` use the **first** domain.
- Full email addresses select their domain explicitly; bare IPv4 domains are normalized to brackets.
- Explicit admin creation bypasses the JIT domain allowlist. The resulting account can log in even when its domain no longer permits new JIT accounts.

```bash
madmail accounts create alice@b.com --password 'secret'
madmail accounts create alice@c.com --password 'different-secret'
madmail accounts create 'alice@[1.1.1.1]' --password 'another-secret'
```

These are separate accounts, even though their localpart is the same.

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
