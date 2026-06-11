# `madmail port submission-tls`

Parent: [`port`](port.md)

Aliases: `submission_tls`.

Manage **Submission TLS (465)** listener port and bind mode. Default port: **465**.

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
madmail port submission-tls status
madmail port submission-tls set 465
madmail port submission-tls local
madmail port submission-tls public
madmail reload
```

## JSON output (`--json`)

```bash
madmail port submission tls --json
```

Success stdout:

```json
{"ok": true, "command": "port submission tls", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-submission-tls).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
