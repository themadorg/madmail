# `madmail certificate autocert status`

Parent: [`certificate`](certificate.md)

Show autocert mode, contact email, and renewal eligibility

## Synopsis

```bash
madmail certificate autocert status [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail certificate autocert status --json
```

Success stdout:

```json
{"ok": true, "command": "certificate autocert status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#certificate-autocert-status).


---
[← `certificate`](certificate.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/certificate.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/certificate.rs)
