# `madmail proxy status`

Parent: [`proxy`](proxy.md)

Show Shadowsocks configuration and client URL.

## Synopsis

```bash
madmail proxy status [OPTIONS]
```

Omitting a subcommand is the same as `status`.

## Notes

When configured, prints port, cipher, password source (config file or DB override), and a ready-to-copy `ss://` client URL. Password values are never printed in human output.

`ws_enabled` and `grpc_enabled` appear in `--json` output when WebSocket or gRPC transports are active.

After changes, run `madmail reload` to apply.

## JSON output (`--json`)

```bash
madmail proxy status --json
```

Success stdout:

```json
{"ok": true, "command": "proxy status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#proxy-status).


---
[← `proxy`](proxy.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/proxy.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/proxy.rs)