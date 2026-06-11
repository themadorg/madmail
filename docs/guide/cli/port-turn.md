# `madmail port turn`

Parent: [`port`](port.md)

Manage **TURN (3478)** listener port and bind mode. Default port: **3478**.

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
madmail port turn status
madmail port turn set 3478
madmail port turn local
madmail port turn public
madmail reload
```

## JSON output (`--json`)

```bash
madmail port turn --json
```

Success stdout:

```json
{"ok": true, "command": "port turn", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-turn).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
