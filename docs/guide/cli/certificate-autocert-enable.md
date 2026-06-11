# `madmail certificate autocert enable`

Parent: [`certificate`](certificate.md)

Turn on autocert mode and store ACME contact email (optional immediate issuance)

## Synopsis

```bash
madmail certificate autocert enable [OPTIONS] --email <EMAIL>
```

## Options

| Option | Description |
|--------|-------------|
| `--email` | ACME contact email (Let's Encrypt account) |
| `--http-listen` | HTTP-01 listener (port 80 must be free when `--obtain` is used) [default: 0.0.0.0:80] |
| `--staging` | Let's Encrypt staging (for tests) |
| `--obtain` | certificate immediately after enabling (needs port 80 free) |
## Examples

```bash
madmail certificate autocert enable --email admin@example.org
```

## Notes

Sets `tls_mode = autocert` in config and stores ACME email. `--obtain` defaults to on.

## JSON output (`--json`)

```bash
madmail certificate autocert enable --json
```

Success stdout:

```json
{"ok": true, "command": "certificate autocert enable", "data": { ... }}
```

Schema: [json-output.md](json-output.md#certificate-autocert-enable).


---
[← `certificate`](certificate.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/certificate.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/certificate.rs)
