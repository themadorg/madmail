# `madmail accounts export`

Parent: [`accounts`](accounts.md)

Export usernames (and hashes) as JSON

## Synopsis

```bash
madmail accounts export [OPTIONS]
```

## Options

| Option | Description |
|--------|-------------|
| `-o`, `--output` | Write to file instead of stdout |
## Examples

```bash
madmail accounts export -o backup.json
```

## JSON output (`--json`)

```bash
madmail accounts export --json
```

Success stdout:

```json
{"ok": true, "command": "accounts export", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-export).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
