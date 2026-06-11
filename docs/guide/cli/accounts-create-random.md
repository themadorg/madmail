# `madmail accounts create-random`

Parent: [`accounts`](accounts.md)

Random account; prints JSON credentials

## Synopsis

```bash
madmail accounts create-random [OPTIONS]
```

## Options

| Option | Description |
|--------|-------------|
| `--json-only` |  |

## Notes

Same as [`create-user`](create-user.md). Prints JSON with a `dclogin` Delta Chat login URI.

## JSON output (`--json`)

```bash
madmail accounts create random --json
```

Success stdout:

```json
{"ok": true, "command": "accounts create random", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-create-random).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
