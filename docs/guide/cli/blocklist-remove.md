# `madmail blocklist remove`

Parent: [`blocklist`](blocklist.md)

Unblock a username

## Synopsis

```bash
madmail blocklist remove [OPTIONS] <USERNAME>
```

## Options

| Option | Description |
|--------|-------------|
| `-y`, `--yes` | Skip confirmation prompt |

## JSON output (`--json`)

```bash
madmail blocklist remove --json
```

Success stdout:

```json
{"ok": true, "command": "blocklist remove", "data": { ... }}
```

Schema: [json-output.md](json-output.md#blocklist-remove).


---
[← `blocklist`](blocklist.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/blocklist_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/blocklist_cmd.rs)
