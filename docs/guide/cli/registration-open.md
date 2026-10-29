# `madmail registration open`

Parent: [`registration`](registration.md)

Allow `/new` when tokens and policy permit. Password-only IMAP and SMTP sign-up stays off while JIT registration is disabled; the registration page then uses `POST /new`. A running server applies this on the next `madmail reload`.

## Synopsis

```bash
madmail registration open [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail registration open --json
```

Success stdout:

```json
{"ok": true, "command": "registration open", "data": { ... }}
```

Schema: [json-output.md](json-output.md#registration-open).


---
[← `registration`](registration.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/registration.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/registration.rs)
