# `madmail sharing disable`

Parent: [`sharing`](sharing.md)

Disable public contact sharing without deleting saved links.

## Synopsis

```bash
madmail sharing disable
madmail --json sharing disable
```

## Notes

Writes `__CONTACT_SHARING_ENABLED__` = `false`. `GET` and `POST /share`, and existing public slug pages, return 404. `sharing.db` is left in place. `list`, `create`, `reserve`, `edit`, and `remove` still work. [`sharing enable`](sharing-enable.md) serves the saved links again.

The next HTTP request picks this up. No restart and no `madmail reload` are required. JSON `reload_required` is `false`.

The command does not print saved URLs.

## JSON output (`--json`)

```bash
madmail sharing disable --json
```

Success stdout:

```json
{"ok": true, "command": "sharing disable", "message": "Contact sharing disabled", "data": { ... }}
```

Schema: [json-output.md](json-output.md#sharing-disable).


---
[← `sharing`](sharing.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/sharing.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/sharing.rs)
