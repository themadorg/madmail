# `madmail registration-tokens list`

Parent: [`registration-tokens`](registration-tokens.md)

List all registration tokens

## Synopsis

```bash
madmail registration-tokens list [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail registration tokens list --json
```

Success stdout:

```json
{"ok": true, "command": "registration tokens list", "data": { ... }}
```

Schema: [json-output.md](json-output.md#registration-tokens-list).


---
[← `registration-tokens`](registration-tokens.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/registration_tokens.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/registration_tokens.rs)
