# `tokens`

Alias for [`registration-tokens`](registration-tokens.md).

```bash
madmail tokens   # same as: madmail registration-tokens
```

All subcommands and flags are identical. See [registration-tokens.md](registration-tokens.md).

## JSON output (`--json`)

```bash
madmail tokens --json
```

Success stdout:

```json
{"ok": true, "command": "tokens", "data": { ... }}
```

Schema: [json-output.md](json-output.md#tokens).


---
[← CLI index](README.md)

[Source: `crates/chatmail/src/ctl/registration_tokens.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/registration_tokens.rs)
