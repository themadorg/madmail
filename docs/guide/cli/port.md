# `port`

Manage listener ports and local-only vs public bind mode for each service.


## Synopsis

```bash
madmail port <status|SERVICE>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Top-level

| Subcommand | Description |
|------------|-------------|
| `status` | Show mode and port for all services |

## Per-service subcommands

Each service supports: `status`, `set <PORT>`, `reset`, `local`, `public`. HTTP and HTTPS also support `enable` and `disable`.

| Service | CLI name |
|---------|----------|
| SMTP | `smtp` |
| Submission | `submission` |
| Submission TLS | `submission-tls` (alias `submission_tls`) |
| IMAP | `imap` |
| IMAP TLS | `imap-tls` (alias `imap_tls`) |
| TURN | `turn` |
| SASL | `sasl` |
| iroh | `iroh` |
| Shadowsocks | `shadowsocks` (alias `ss`) |
| HTTP | `http` |
| HTTPS | `https` |

## Examples

```bash
madmail port status
madmail port smtp set 2525
madmail port https local
madmail port imap public
madmail reload
```

## Subcommand pages

- [`status`](port-status.md) — `madmail port status`
- [`smtp`](port-smtp.md) — `madmail port smtp …`
- [`submission`](port-submission.md) — `madmail port submission …`
- [`submission-tls`](port-submission-tls.md) — `madmail port submission-tls …`
- [`imap`](port-imap.md) — `madmail port imap …`
- [`imap-tls`](port-imap-tls.md) — `madmail port imap-tls …`
- [`turn`](port-turn.md) — `madmail port turn …`
- [`sasl`](port-sasl.md) — `madmail port sasl …`
- [`iroh`](port-iroh.md) — `madmail port iroh …`
- [`shadowsocks`](port-shadowsocks.md) — `madmail port shadowsocks …`
- [`http`](port-http.md) — `madmail port http …`
- [`https`](port-https.md) — `madmail port https …`

## Default ports

| Service | CLI | Default |
|---------|-----|--------|
| SMTP (25) | `smtp` | 25 |
| Submission (587) | `submission` | 587 |
| Submission TLS (465) | `submission-tls` (`submission_tls`) | 465 |
| IMAP (143) | `imap` | 143 |
| IMAP TLS (993) | `imap-tls` (`imap_tls`) | 993 |
| TURN (3478) | `turn` | 3478 |
| SASL (24) | `sasl` | 24 |
| Iroh (3340) | `iroh` | 3340 |
| Shadowsocks (8388) | `shadowsocks` (`ss`) | 8388 |
| HTTP (80) | `http` | 80 |
| HTTPS (443) | `https` | 443 |

Per-service docs: [`smtp`](port-smtp.md), [`submission`](port-submission.md), [`submission-tls`](port-submission-tls.md), [`imap`](port-imap.md), [`imap-tls`](port-imap-tls.md), [`turn`](port-turn.md), [`sasl`](port-sasl.md), [`iroh`](port-iroh.md), [`shadowsocks`](port-shadowsocks.md), [`http`](port-http.md), [`https`](port-https.md).

## JSON output (`--json`)

```bash
madmail port --json
```

Success stdout:

```json
{"ok": true, "command": "port", "data": { ... }}
```

Schema: [json-output.md](json-output.md#port).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/port.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/port.rs)
