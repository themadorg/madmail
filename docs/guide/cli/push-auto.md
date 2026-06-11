# `madmail push auto`

Parent: [`push`](push.md)

Auto mode (default): enabled until 5 consecutive notification failures

## Synopsis

```bash
madmail push auto [OPTIONS]
```


## Notes

Default mode: enabled until 5 consecutive notification-proxy failures.

After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail push auto --json
```

Success stdout:

```json
{"ok": true, "command": "push auto", "data": { ... }}
```

Schema: [json-output.md](json-output.md#push-auto).


---
[← `push`](push.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/push.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/push.rs)
