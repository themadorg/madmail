# `madmail accounts create`

Parent: [`accounts`](accounts.md)

Create login + maildir + quota row, and print a one-time Delta Chat `dclogin:` URI (IMAP + SMTP host/port/TLS hints).

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
- The printed `dclogin:` URI is what Delta Chat needs. Do **not** hand-build a link from username and password only — current Delta Chat uses `ih`/`ip`/`is`/`sh`/`sp`/`ss` for SMTP. A password-only URI often IMAP-logins but cannot send (including Saved Messages).
- The password is not stored. To reprint the URI later: [`madmail accounts dclogin`](accounts-dclogin.md) with the same password.

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
