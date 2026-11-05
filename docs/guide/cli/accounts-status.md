# `madmail accounts status`

Parent: [`accounts`](accounts.md)

Credentials and storage summary

The registration rows print the stored switches one by one. `First-login sign-up (IMAP/SMTP)` (JSON `password_signup`) is the combined result: on only when registration is open, JIT registration is enabled and no registration token is required. It reads the database, so a running server applies a change after `madmail reload`.

## Synopsis

```bash
madmail accounts status [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail accounts status --json
```

Success stdout:

```json
{"ok": true, "command": "accounts status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-status).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
