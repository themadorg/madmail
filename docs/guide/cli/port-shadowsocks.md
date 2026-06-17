# `madmail port shadowsocks`

Parent: [`port`](port.md)

Aliases: `ss`.

Manage **Shadowsocks (8388)** listener port and bind mode. Default port: **8388**.

Enable/disable, cipher, password, and client URL: [`proxy`](proxy.md).

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
madmail port shadowsocks status
madmail port shadowsocks set 8388
madmail port shadowsocks local
madmail port shadowsocks public
madmail reload
```

## JSON output (`--json`)

```bash
madmail port shadowsocks --json
```

Success stdout:

```json
{"ok": true, "command": "port shadowsocks", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port-shadowsocks).


---
[← `port`](port.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
