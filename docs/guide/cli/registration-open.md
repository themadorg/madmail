# `madmail registration open`

Parent: [`registration`](registration.md)

Allow `/new` registration when tokens/policy permit

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
