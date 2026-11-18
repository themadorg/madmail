# `madmail sharing enable`

Parent: [`sharing`](sharing.md)

Enable public contact sharing. Saved links stay in `sharing.db`.

## Synopsis

```bash
madmail sharing enable
madmail --json sharing enable
```

## Notes

Writes `__CONTACT_SHARING_ENABLED__` = `true`. That overrides `enable_contact_sharing` in the config file. The next request to `/share` or a public contact page serves saved links. No restart and no `madmail reload` are required. JSON `reload_required` is `false`.

The command does not print saved URLs.

## JSON output (`--json`)

```bash
madmail sharing enable --json
```

Success stdout:

```json
{"ok": true, "command": "sharing enable", "message": "Contact sharing enabled", "data": { ... }}
```

Schema: [json-output.md](json-output.md#sharing-enable).


---
[← `sharing`](sharing.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/sharing.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/sharing.rs)
