# `madmail port imap-tls`

Parent: [`port`](port.md)

Aliases: `imap_tls`.

Manage **IMAP TLS (993)** listener port and bind mode. Default port: **993**.

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
madmail port imap-tls status
madmail port imap-tls set 993
madmail port imap-tls local
madmail port imap-tls public
madmail reload
```

## JSON output (`--json`)

```bash
madmail port imap tls --json
```

Success stdout:

```json
{"ok": true, "command": "port imap tls", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-imap-tls).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
