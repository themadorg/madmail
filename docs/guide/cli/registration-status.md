# `madmail registration status`

Parent: [`registration`](registration.md)

Show open/closed

## Synopsis

```bash
madmail registration status [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail registration status --json
```

Success stdout:

```json
{"ok": true, "command": "registration status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#registration-status).


---
[← `registration`](registration.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/registration.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/registration.rs)
