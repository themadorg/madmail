# `madmail port https`

Parent: [`port`](port.md)

Manage **HTTPS (443)** listener port and bind mode. Default port: **443**.

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
madmail port https status
madmail port https set 443
madmail port https local
madmail port https public
madmail reload
```

## JSON output (`--json`)

```bash
madmail port https --json
```

Success stdout:

```json
{"ok": true, "command": "port https", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-https).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
