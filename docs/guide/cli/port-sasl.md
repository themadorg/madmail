# `madmail port sasl`

Parent: [`port`](port.md)

Manage **SASL (24)** listener port and bind mode. Default port: **24**.

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
madmail port sasl status
madmail port sasl set 24
madmail port sasl local
madmail port sasl public
madmail reload
```

## JSON output (`--json`)

```bash
madmail port sasl --json
```

Success stdout:

```json
{"ok": true, "command": "port sasl", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-sasl).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
