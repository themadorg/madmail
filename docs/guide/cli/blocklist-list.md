# `madmail blocklist list`

Parent: [`blocklist`](blocklist.md)

List all blocked users

## Synopsis

```bash
madmail blocklist list [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail blocklist list --json
```

Success stdout:

```json
{"ok": true, "command": "blocklist list", "data": { ... }}
```

Schema: [json-output.md](json-output.md#blocklist-list).


---
[← `blocklist`](blocklist.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/blocklist_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/blocklist_cmd.rs)
