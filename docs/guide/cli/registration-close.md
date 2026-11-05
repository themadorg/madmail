# `madmail registration close`

Parent: [`registration`](registration.md)

Block new public registrations, including first-login IMAP and SMTP account creation. `POST /new` still accepts a valid invite token. Accounts created with `accounts create` or `create-user` keep logging in. A running server applies this on the next `madmail reload`.

## Synopsis

```bash
madmail registration close [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail registration close --json
```

Success stdout:

```json
{"ok": true, "command": "registration close", "data": { ... }}
```

Schema: [json-output.md](json-output.md#registration-close).


---
[← `registration`](registration.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/registration.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/registration.rs)
