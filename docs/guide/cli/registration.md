# `registration`

Open or close public account registration (`__REGISTRATION_OPEN__`).

`close` blocks new accounts from `POST /new` unless the request includes a valid invite token, and it blocks a first IMAP or SMTP login from creating an account. `open` allows `POST /new` again. Password-only IMAP and SMTP sign-up also requires JIT registration (`__JIT_REGISTRATION_ENABLED__`) to be enabled. With JIT disabled, the registration page creates the account through `POST /new` instead of a first login.

Accounts created with [`accounts create`](accounts.md) or `create-user` keep logging in. A running server reads both switches at startup and on `madmail reload`. IMAP and SMTP do not accept an invite token; the token is presented to `POST /new`, and that account then logs in as an existing account.


## Synopsis

```bash
madmail registration <open|close|status>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `open` | Allow `/new` when tokens and policy permit. Password-only IMAP/SMTP sign-up stays off while JIT is disabled |
| `close` | Block new public registrations, including IMAP/SMTP first-login creation. A valid invite token still works on `POST /new` |
| `status` | Show open/closed |

## Examples

```bash
madmail registration status
madmail registration open
madmail registration close
```

Combine with [registration-tokens](registration-tokens.md) for invite-only registration while closed.

## Subcommand pages

- [`close`](registration-close.md) — `madmail registration close`
- [`open`](registration-open.md) — `madmail registration open`
- [`status`](registration-status.md) — `madmail registration status`

## JSON output (`--json`)

```bash
madmail registration --json
```

Success stdout:

```json
{"ok": true, "command": "registration", "data": { ... }}
```

Schema: [json-output.md](json-output.md#registration).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/registration.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/registration.rs)
