# `madmail port submission`

Parent: [`port`](port.md)

Manage **Submission (587)** listener port and bind mode. Default port: **587**.

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
madmail port submission status
madmail port submission set 587
madmail port submission local
madmail port submission public
madmail reload
```

## JSON output (`--json`)

```bash
madmail port submission --json
```

Success stdout:

```json
{"ok": true, "command": "port submission", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-submission).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
