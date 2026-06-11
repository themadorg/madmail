# `madmail push off`

Parent: [`push`](push.md)

Force push off

## Synopsis

```bash
madmail push off [OPTIONS]
```


After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail push off --json
```

Success stdout:

```json
{"ok": true, "command": "push off", "data": { ... }}
```

Schema: [json-output.md](json-output.md#push-off).


---
[← `push`](push.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/push.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/push.rs)
