# `madmail registration-tokens create`

Parent: [`registration-tokens`](registration-tokens.md)

Create a new registration token

## Synopsis

```bash
madmail registration-tokens create [OPTIONS]
```

## Options

| Option | Description |
|--------|-------------|
| `--token` | <TOKEN> |
| `--max-uses` | [default: 1] |
| `--comment` | [default: ""] |
| `--expires` | Expiration duration (e.g. `72h`, `168h`) |
## Examples

```bash
madmail registration-tokens create --max-uses 5 --expires 72h --comment "invite"
```

## JSON output (`--json`)

```bash
madmail registration tokens create --json
```

Success stdout:

```json
{"ok": true, "command": "registration tokens create", "data": { ... }}
```

Schema: [json-output.md](json-output.md#registration-tokens-create).


---
[← `registration-tokens`](registration-tokens.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/registration_tokens.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/registration_tokens.rs)
