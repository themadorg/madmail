# Global CLI flags

These flags are available on **every** `madmail` subcommand.

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |
| `--json` | — | — | off | Emit machine-readable JSON on stdout (no decorative text or QR codes) |

## JSON output

Add `--json` to any subcommand for scripting and automation:

```bash
madmail --json accounts status
madmail federation list --json
```

- **Success:** `{"ok":true,"command":"…","data":{…}}` on stdout (optional `message` field).
- **Failure:** `{"ok":false,"error":"…"}` on stderr, exit code 1.

Full schemas per command: [json-output.md](json-output.md).

## Path auto-detection

When flags are omitted, madmail detects development layouts:

- **Config:** `./data/chatmail.toml` if it exists, otherwise `/etc/madmail/madmail.conf`
- **State dir:** `./data` if it contains `chatmail.db` or `admin_token`, otherwise `/var/lib/madmail`

## systemd compatibility

Madmail systemd units historically pass state directory as `--libexec`:

```ini
ExecStart=/usr/local/bin/madmail --config /etc/madmail/madmail.conf run --libexec /var/lib/madmail
```

`--libexec` is an alias for `--state-dir`.

## Environment variables

```bash
export CHATMAIL_CONFIG=/etc/madmail/madmail.conf
export CHATMAIL_STATE_DIR=/var/lib/madmail
madmail status
```

---
[← CLI index](README.md)

[Source: `crates/chatmail-config/src/cli.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail-config/src/cli.rs)
