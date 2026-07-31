# `madmail openrelay enable`

Parent: [`openrelay`](openrelay.md)

Allow non-local RCPT on unauthenticated inbound SMTP (open-relay-class; lab/special only).

## Synopsis

```bash
madmail openrelay enable [OPTIONS]
```

Then apply on a running server:

```bash
madmail reload
```

## JSON output (`--json`)

```bash
madmail openrelay enable --json
```

Success stdout:

```json
{"ok": true, "command": "openrelay enable", "message": "...", "data": { ... }}
```

Schema: [json-output.md](json-output.md#openrelay-enable--disable).


---
[← `openrelay`](openrelay.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/openrelay.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/openrelay.rs)
