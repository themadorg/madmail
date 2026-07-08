# `madmail port http`

Parent: [`port`](port.md)

Manage **HTTP (80)** listener port and bind mode. Default port: **80**.

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show current port, enabled state, and local/public mode |
| `set <PORT>` | Set port number (`1`–`65535`) |
| `reset` | Clear DB override (revert to config default) |
| `local` | Listen on localhost only |
| `public` | Listen on all interfaces (`0.0.0.0`) |
| `enable` | Start the plain HTTP listener |
| `disable` | Stop the plain HTTP listener |

## Examples

```bash
madmail port http status
madmail port http set 80
madmail port http local
madmail port http public
madmail port http disable
madmail port http enable
madmail reload
```

## Notes

Disabling HTTP stops the plain listener only. Run `madmail reload` after `enable` / `disable` so the running server picks up the change. HTTPS is controlled separately via [`port https`](port-https.md).

## JSON output (`--json`)

```bash
madmail port http --json
```

Success stdout:

```json
{"ok": true, "command": "port http", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-http).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
