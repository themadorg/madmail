# `madmail registration-tokens status`

Parent: [`registration-tokens`](registration-tokens.md)

Show detailed status for a specific token

## Synopsis

```bash
madmail registration-tokens status [OPTIONS] <TOKEN>
```

## JSON output (`--json`)

```bash
madmail registration tokens status --json
```

Success stdout:

```json
{"ok": true, "command": "registration tokens status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#registration-tokens-status).


---
[← `registration-tokens`](registration-tokens.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/registration_tokens.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/registration_tokens.rs)
