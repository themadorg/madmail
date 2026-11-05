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

## What is detected

Uninstall reads every `madmail*` / `chatmail*` unit in the systemd unit directories.
From each unit it takes `User=`, the state directory (`WorkingDirectory=`), the
binary (first word of `ExecStart=`) and the config directory (parent of the
`--config` path in `ExecStart=`), so a custom-path `install --system` (for example
`--config-dir /var/xyzzy/etc/madmail`) has its config, secrets and TLS key removed.
It also finds `/etc/madmail*`, `/etc/chatmail*`, `/var/lib/madmail*`,
`/var/lib/chatmail*`, family binaries in `/usr/local/bin` and `/usr/bin`, and the
`--state-dir` given to uninstall.

Limits: a config directory with only one path component (such as `/etc`) is never
derived from a unit. If the unit file was removed or edited by hand, custom config
and state directories outside the default locations are not found; pass
`--state-dir` and remove the config directory yourself. Check the plan with
`--dry-run` first.

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
