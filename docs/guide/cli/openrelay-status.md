# `madmail openrelay status`

Parent: [`openrelay`](openrelay.md)

Show whether inbound remote RCPT (open relay) is allowed.

## Synopsis

```bash
madmail openrelay status [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail openrelay status --json
```

Success stdout:

```json
{"ok": true, "command": "openrelay status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#openrelay-status).


---
[← `openrelay`](openrelay.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/openrelay.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/openrelay.rs)
