# `install`

Bootstrap a new madmail server: config, TLS, SQLite DB, optional systemd unit and service user.


## Synopsis

```bash
madmail install [OPTIONS]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Overview

`install` writes configuration, certificates, and initial database state. It is **Madmail-compatible**.

**Full reference:** [Native install guide](../install.md)

## Quick examples

```bash
# Public IP relay (self-signed TLS)
sudo madmail install --simple --ip 203.0.113.50 --lang en

# DNS domain with Let's Encrypt (obtains cert during install; port 80 must be free)
sudo madmail install --simple --domain example.org --acme-email admin@example.org --lang en

# Two-step: obtain cert first, then finish install
sudo madmail install --simple --domain example.org --acme-email admin@example.org --cert-only
sudo madmail install --simple --domain example.org --tls-mode file --no-obtain-certificate --lang en

# Local dev (no root)
madmail install --simple --ip 127.0.0.1   --config-dir /tmp/mm --state-dir /tmp/sd

# Windows (elevated): ProgramData defaults, service + firewall
madmail install --simple --ip 203.0.113.50 --tls-mode self_signed --lang en `
  --install-service --start-service --firewall
```

On **Windows**, default install paths are `%ProgramData%\Madmail\config` and `%ProgramData%\Madmail\data` (not `/etc` / `/var/lib`). Use the [Windows packaging guide](../../../packaging/windows/README.md) for the Inno Setup wizard and tray helper.

## Key flags

| Flag | Description |
|------|-------------|
| `-s`, `--simple` | Quick setup (`--ip` or `--domain` required) |
| `-n`, `--non-interactive` | Script install (requires `--domain` without `--simple`) |
| `--ip`, `--domain`, `--hostname` | Server identity |
| `--config-dir`, `--state-dir` | Override FHS / ProgramData paths |
| `--install-service` | Register Windows service after install (no-op notice on Unix) |
| `--start-service` | Start Windows service after install |
| `--firewall` | Open Windows Firewall rules for mail/HTTP ports |
| `--tls-mode` | `autocert`, `file`, or `self_signed` |
| `--acme-email`, `--auto-ip-cert`, `--obtain-certificate`, `--no-obtain-certificate`, `--cert-only`, `--http-listen` | TLS issuance |
| `--lang` | UI language: `en`, `fa`, `ru`, `es` |
| `--skip-systemd`, `--skip-user` | Container / CI installs |
| `--dry-run` | Preview resolved paths without writing |

> The global `--config` flag does **not** control where `install` writes files; use `--config-dir` instead.


## Related

- [Native install guide](../install.md)
- [Docker guide](../docker.md)

## JSON output (`--json`)

```bash
madmail install --json
```

Success stdout:

```json
{"ok": true, "command": "install", "data": { ... }}
```

Schema: [json-output.md](json-output.md#install).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/install/mod.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/install/mod.rs)
