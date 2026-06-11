# `madmail sharing remove`

Parent: [`sharing`](sharing.md)

Remove a share link

## Synopsis

```bash
madmail sharing remove [OPTIONS] <SLUG>
```


## Notes

Alias: `madmail sharing delete <SLUG>`.

## JSON output (`--json`)

```bash
madmail sharing remove --json
```

Success stdout:

```json
{"ok": true, "command": "sharing remove", "data": { ... }}
```

Schema: [json-output.md](json-output.md#sharing-remove).


---
[← `sharing`](sharing.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/sharing.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/sharing.rs)
