# `admin-token`

Display the admin API bearer token and login URL. Reads `admin_token` from the state directory and builds the URL from DB settings (`__SMTP_HOSTNAME__`, `__HTTPS_PORT__`, `__ADMIN_PATH__`).


## Synopsis

```bash
madmail admin-token [--raw] [--no-qr]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Options

| Flag | Description |
|------|-------------|
| `--raw` | Print only the token (for scripts: `TOKEN=$(madmail admin-token --raw)`) |
| `--no-qr` | Skip the terminal QR code for admin login |

## Examples

```bash
madmail admin-token
madmail admin-token --raw
```

## Notes

- Requires read access to `{state_dir}/admin_token`.
- The token grants **full** admin API access; rotate it regularly on production servers.

## JSON output (`--json`)

```bash
madmail admin token --json
```

Success stdout:

```json
{"ok": true, "command": "admin token", "data": { ... }}
```

Schema: [json-output.md](json-output.md#admin-token).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/admin_token.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/admin_token.rs)
