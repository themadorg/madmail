# `madmail certificate get`

Parent: [`certificate`](certificate.md)

Obtain certificate if missing or expiring within 30 days

## Synopsis

```bash
madmail certificate get [OPTIONS]
```

## Options

| Option | Description |
|--------|-------------|
| `--domain` | DNS name (default: `primary_domain` from config) |
| `--email` | ACME contact email (default: `admin@<domain>`) |
| `--http-listen` | HTTP-01 listener (port 80 must be free) [default: 0.0.0.0:80] |
| `--staging` | Let's Encrypt staging (for tests) |
| `--force` | Force issuance even if current cert is still valid |
## Examples

```bash
madmail certificate get --email admin@example.org
```

## Notes

Issues only if cert is missing or expires within 30 days (unless `--force`). Port 80 must be free for HTTP-01.

## JSON output (`--json`)

```bash
madmail certificate get --json
```

Success stdout:

```json
{"ok": true, "command": "certificate get", "data": { ... }}
```

Schema: [json-output.md](json-output.md#certificate-get).


---
[← `certificate`](certificate.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/certificate.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/certificate.rs)
