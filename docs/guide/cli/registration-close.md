# `madmail registration close`

Parent: [`registration`](registration.md)

Block new registrations

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
