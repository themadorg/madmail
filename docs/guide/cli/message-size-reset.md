# `madmail message-size reset`

Parent: [`message-size`](message-size.md)

Clear DB overrides (revert to config file / default)

## Synopsis

```bash
madmail message-size reset [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail message size reset --json
```

Success stdout:

```json
{"ok": true, "command": "message size reset", "data": { ... }}
```

Schema: [json-output.md](json-output.md#message-size-reset).


---
[← `message-size`](message-size.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/message_size.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/message_size.rs)
