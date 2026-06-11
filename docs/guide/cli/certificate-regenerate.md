# `madmail certificate regenerate`

Parent: [`certificate`](certificate.md)

Force new certificate issuance

## Synopsis

```bash
madmail certificate regenerate [OPTIONS]
```

## Options

| Option | Description |
|--------|-------------|
| `--domain` | DNS name (default: `primary_domain` from config) |
| `--email` | ACME contact email (default: `admin@<domain>`) |
| `--http-listen` | HTTP-01 listener (port 80 must be free) [default: 0.0.0.0:80] |
| `--staging` | Let's Encrypt staging (for tests) |
| `--force` | issuance on `get` even if current cert is still valid |

## JSON output (`--json`)

```bash
madmail certificate regenerate --json
```

Success stdout:

```json
{"ok": true, "command": "certificate regenerate", "data": { ... }}
```

Schema: [json-output.md](json-output.md#certificate-regenerate).


---
[← `certificate`](certificate.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/certificate.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/certificate.rs)
