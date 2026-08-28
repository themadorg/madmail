# `madmail accounts dclogin`

Parent: [`accounts`](accounts.md)

Print a Delta Chat `dclogin:` URI for an **existing** account. Same shape as [`create-user`](create-user.md) / `POST /new` (`ih`/`ip`/`is`/`sh`/`sp`/`ss`/`ic`).

The password is **not** stored. You must pass the account password (`-p` or stdin). It is checked against the stored hash before the URI is printed.

## Synopsis

```bash
madmail accounts dclogin [OPTIONS] <USERNAME>
```

## Options

| Option | Description |
|--------|-------------|
| `-p`, `--password` | Password (prompted on stdin if omitted) |

## Examples

```bash
madmail accounts dclogin alice@example.org --password 'secret'
madmail accounts dclogin alice@example.org --json
```

## Notes

- Usernames without `@` are expanded using the registration domain from config.
- A URI that is only `dclogin:user@host/?p=…` (no IMAP/SMTP host and TLS hints) is not enough for current Delta Chat: IMAP can succeed while SMTP send / Saved Messages fail.
- Does not create an account. For a new custom username, use [`accounts create`](accounts-create.md) (it prints this URI once).

## JSON output (`--json`)

```bash
madmail accounts dclogin alice@example.org --password 'secret' --json
```

Success stdout:

```json
{"ok": true, "command": "accounts dclogin", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-dclogin).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
