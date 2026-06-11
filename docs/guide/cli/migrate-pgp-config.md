# `migrate-pgp-config`

Migrate submission PGP policy settings in the config file.

> **Status:** This command is defined in the CLI but **not yet implemented** in madmail-rs. Running it prints a not-implemented error. See [CLI tools (TDD)](../../TDD/14-cli-tools.md) for the parity matrix.

## Synopsis

```bash
madmail migrate-pgp-config
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |

## JSON output (`--json`)

Not implemented — `--json` returns an error envelope. See [json-output.md](json-output.md#planned-commands).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `internal/cli/ctl/migrate_pgp_config.go`](https://github.com/themadorg/madmail/blob/main/internal/cli/ctl/migrate_pgp_config.go)
