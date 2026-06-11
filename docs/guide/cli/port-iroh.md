# `madmail port iroh`

Parent: [`port`](port.md)

Manage **Iroh (3340)** listener port and bind mode. Default port: **3340**.

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
madmail port iroh status
madmail port iroh set 3340
madmail port iroh local
madmail port iroh public
madmail reload
```

## JSON output (`--json`)

```bash
madmail port iroh --json
```

Success stdout:

```json
{"ok": true, "command": "port iroh", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-iroh).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
