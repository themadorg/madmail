# `madmail push status`

Parent: [`push`](push.md)

Show mode, runtime status, and failure counters

## Synopsis

```bash
madmail push status [OPTIONS]
```


## Notes

Run `madmail reload` after mode changes to refresh IMAP `XDELTAPUSH` advertisement.

After changes, run `madmail reload` (or restart) to apply.

## JSON output (`--json`)

```bash
madmail push status --json
```

Success stdout:

```json
{"ok": true, "command": "push status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#push-status).


---
[← `push`](push.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/push.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/push.rs)
