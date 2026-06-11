# `madmail accounts status`

Parent: [`accounts`](accounts.md)

Credentials and storage summary

## Synopsis

```bash
madmail accounts status [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail accounts status --json
```

Success stdout:

```json
{"ok": true, "command": "accounts status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-status).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
