# `madmail sharing status`

Parent: [`sharing`](sharing.md)

Show whether public contact sharing is enabled.

## Synopsis

```bash
madmail sharing status
madmail --json sharing status
```

## Notes

The effective value is `__CONTACT_SHARING_ENABLED__` when that row exists. Otherwise it is `enable_contact_sharing` from the config file. `status` does not write a row.

JSON `source` is `database` or `config`. `config_enabled` is the file fallback. `reload_required` is `false`. `file_change_requires_restart` is true only when `source` is `config`.

A database row applies on the next HTTP request. Human output then says no restart is required. With no saved row, human output says the running server keeps the file value from startup and that editing the file needs a restart.

## JSON output (`--json`)

```bash
madmail sharing status --json
```

Success stdout:

```json
{"ok": true, "command": "sharing status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#sharing-status).


---
[← `sharing`](sharing.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/sharing.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/sharing.rs)
