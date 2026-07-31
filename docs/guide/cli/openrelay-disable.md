# `madmail openrelay disable`

Parent: [`openrelay`](openrelay.md)

Deny non-local RCPT on unauthenticated inbound SMTP (default-safe).

## Synopsis

```bash
madmail openrelay disable [OPTIONS]
```

Then apply on a running server:

```bash
madmail reload
```

## JSON output (`--json`)

```bash
madmail openrelay disable --json
```

Success stdout:

```json
{"ok": true, "command": "openrelay disable", "message": "...", "data": { ... }}
```

Schema: [json-output.md](json-output.md#openrelay-enable--disable).


---
[← `openrelay`](openrelay.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/openrelay.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/openrelay.rs)
