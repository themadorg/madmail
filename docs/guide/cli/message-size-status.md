# `madmail message-size status`

Parent: [`message-size`](message-size.md)

Show effective limit and DB overrides

## Synopsis

```bash
madmail message-size status [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail message size status --json
```

Success stdout:

```json
{"ok": true, "command": "message size status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#message-size-status).


---
[← `message-size`](message-size.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/message_size.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/message_size.rs)
