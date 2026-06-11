# `queue`

Outbound delivery queue management.

> **Status:** This command is defined in the CLI but **not yet implemented** in madmail-rs. Running it prints a not-implemented error. See [CLI tools (TDD)](../../TDD/14-cli-tools.md) for the parity matrix.

## Synopsis

```bash
madmail queue
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |




## Related

- [tasks](tasks.md)
- Admin API `/admin/queue`

## JSON output (`--json`)

Not implemented — `--json` returns an error envelope. See [json-output.md](json-output.md#planned-commands).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `internal/cli/ctl/queue.go`](https://github.com/themadorg/madmail/blob/main/internal/cli/ctl/queue.go)
