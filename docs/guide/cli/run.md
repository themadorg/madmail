# `run`

Start the mail server (SMTP, IMAP, HTTP, federation, TURN, …). This is the **default** when you invoke `madmail` with no subcommand.


## Synopsis

```bash
madmail [OPTIONS] run
madmail [OPTIONS]   # same as run
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Description

Starts all configured listeners and background tasks. Use systemd in production:

```bash
sudo systemctl enable --now madmail
```

For local development with custom paths:

```bash
madmail --config ./data/chatmail.toml run --libexec ./data
```

## Options

No command-specific flags. See [global flags](global-flags.md).

## Notes

- Logging verbosity is controlled in the config file (`log`, `debug`), not CLI flags.
- Maintenance tasks (`prune-old-messages`, certificate renewal, …) run on a schedule while the server is active.

## JSON output (`--json`)

```bash
madmail run --json
```

Success stdout:

```json
{"ok": true, "command": "run", "data": { ... }}
```

Schema: [json-output.md](json-output.md#run).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/boot.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/boot.rs)
