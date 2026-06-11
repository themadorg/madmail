# `madmail message-size set`

Parent: [`message-size`](message-size.md)

Set both limits (e.g. `100M`, `1G`)

## Synopsis

```bash
madmail message-size set [OPTIONS] <SIZE>
```

## Examples

```bash
madmail message-size set 100M
```

After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail message size set --json
```

Success stdout:

```json
{"ok": true, "command": "message size set", "data": { ... }}
```

Schema: [json-output.md](json-output.md#message-size-set).


---
[← `message-size`](message-size.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/message_size.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/message_size.rs)
