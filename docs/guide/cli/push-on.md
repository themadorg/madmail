# `madmail push on`

Parent: [`push`](push.md)

Force push on (no auto-disable)

## Synopsis

```bash
madmail push on [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail push on --json
```

Success stdout:

```json
{"ok": true, "command": "push on", "data": { ... }}
```

Schema: [json-output.md](json-output.md#push-on).


---
[← `push`](push.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/push.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/push.rs)
