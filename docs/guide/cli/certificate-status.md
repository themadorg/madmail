# `madmail certificate status`

Parent: [`certificate`](certificate.md)

Show certificate management mode and validity

## Synopsis

```bash
madmail certificate status [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail certificate status --json
```

Success stdout:

```json
{"ok": true, "command": "certificate status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#certificate-status).


---
[← `certificate`](certificate.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/certificate.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/certificate.rs)
