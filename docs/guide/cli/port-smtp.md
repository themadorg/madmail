# `madmail port smtp`

Parent: [`port`](port.md)

Manage **SMTP (25)** listener port and bind mode. Default port: **25**.

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show current port and local/public mode |
| `set <PORT>` | Set port number (`1`–`65535`) |
| `reset` | Clear DB override (revert to config default) |
| `local` | Listen on localhost only |
| `public` | Listen on all interfaces (`0.0.0.0`) |

## Examples

```bash
madmail port smtp status
madmail port smtp set 25
madmail port smtp local
madmail port smtp public
madmail reload
```

## JSON output (`--json`)

```bash
madmail port smtp --json
```

Success stdout:

```json
{"ok": true, "command": "port smtp", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-smtp).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
