# `certificate`

TLS certificate management: Let's Encrypt HTTP-01 issuance, status, and in-process autocert renewal.


## Synopsis

```bash
madmail certificate <get|regenerate|status|autocert>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

### `get`

Obtain a certificate if missing or expiring within 30 days.

```bash
madmail certificate get [--domain DOMAIN] [--email EMAIL] [--http-listen ADDR] [--staging] [--force]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--domain` | `primary_domain` from config | DNS name for the cert |
| `--email` | `admin@<domain>` | ACME account contact |
| `--http-listen` | `0.0.0.0:80` | HTTP-01 listener (port 80 must be free) |
| `--staging` | off | Use Let's Encrypt staging |
| `--force` | off | Issue even if current cert is still valid |

### `regenerate`

Force new certificate issuance (same flags as `get`).

### `status`

Show TLS mode and certificate validity.

### `autocert`

| Subcommand | Description |
|------------|-------------|
| `autocert enable --email EMAIL` | Enable in-process auto-renewal (`tls_mode = autocert`) |
| `autocert status` | Show autocert mode and renewal eligibility |

`autocert enable` flags: `--http-listen`, `--staging`, `--obtain` (default on).

## Examples

```bash
madmail certificate status
madmail certificate get --email admin@example.org
madmail certificate regenerate --force
madmail certificate autocert enable --email admin@example.org
```

## Subcommand pages

- [`autocert`](certificate-autocert.md) — `madmail certificate autocert`
- [`autocert enable`](certificate-autocert-enable.md) — `madmail certificate autocert enable`
- [`autocert status`](certificate-autocert-status.md) — `madmail certificate autocert status`
- [`get`](certificate-get.md) — `madmail certificate get`
- [`regenerate`](certificate-regenerate.md) — `madmail certificate regenerate`
- [`status`](certificate-status.md) — `madmail certificate status`

## JSON output (`--json`)

```bash
madmail certificate --json
```

Success stdout:

```json
{"ok": true, "command": "certificate", "data": { ... }}
```

Schema: [json-output.md](json-output.md#certificate).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/certificate.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/certificate.rs)
