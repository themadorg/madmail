# `madmail port imap`

Parent: [`port`](port.md)

Manage **IMAP (143)** listener port and bind mode. Default port: **143**.

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
madmail port imap status
madmail port imap set 143
madmail port imap local
madmail port imap public
madmail reload
```

## JSON output (`--json`)

```bash
madmail port imap --json
```

Success stdout:

```json
{"ok": true, "command": "port imap", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-imap).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
