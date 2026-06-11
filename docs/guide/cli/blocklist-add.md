# `madmail blocklist add`

Parent: [`blocklist`](blocklist.md)

Block a username from re-registration

## Synopsis

```bash
madmail blocklist add [OPTIONS] <USERNAME> [REASON]
```

## Examples

```bash
madmail blocklist add bad@x.org "manual block"
```

## JSON output (`--json`)

```bash
madmail blocklist add --json
```

Success stdout:

```json
{"ok": true, "command": "blocklist add", "data": { ... }}
```

Schema: [json-output.md](json-output.md#blocklist-add).


---
[← `blocklist`](blocklist.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/blocklist_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/blocklist_cmd.rs)
