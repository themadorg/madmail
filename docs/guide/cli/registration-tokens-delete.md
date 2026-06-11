# `madmail registration-tokens delete`

Parent: [`registration-tokens`](registration-tokens.md)

Delete a registration token

## Synopsis

```bash
madmail registration-tokens delete [OPTIONS] <TOKEN>
```

## JSON output (`--json`)

```bash
madmail registration tokens delete --json
```

Success stdout:

```json
{"ok": true, "command": "registration tokens delete", "data": { ... }}
```

Schema: [json-output.md](json-output.md#registration-tokens-delete).


---
[← `registration-tokens`](registration-tokens.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/registration_tokens.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/registration_tokens.rs)
