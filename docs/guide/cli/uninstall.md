# `uninstall`

Remove a madmail installation: systemd unit, binary, service user, config, and/or data.


## Synopsis

```bash
madmail uninstall [OPTIONS]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Options

| Flag | Description |
|------|-------------|
| `--force` | Skip confirmation prompts |
| `--keep-data` | Keep mail data, databases, state directory |
| `--keep-user` | Keep service user and group |
| `--keep-config` | Keep configuration files |
| `--keep-binary` | Keep server binary |
| `--dry-run` | Show what would be removed |
| `--log-file` | Log path (default: `/var/log/madmail-uninstall.log`) |

## Examples

```bash
sudo madmail uninstall --dry-run
sudo madmail uninstall --keep-data
sudo madmail uninstall --force
```

## JSON output (`--json`)

```bash
madmail uninstall --json
```

Success stdout:

```json
{"ok": true, "command": "uninstall", "data": { ... }}
```

Schema: [json-output.md](json-output.md#uninstall).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/uninstall.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/uninstall.rs)
