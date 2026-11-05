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
binary (first word of `ExecStart=`) and the config file (the `--config` path in
`ExecStart=`). It also finds `/etc/madmail*`, `/etc/chatmail*`, `/var/lib/madmail*`,
`/var/lib/chatmail*`, family binaries in `/usr/local/bin` and `/usr/bin`, and the
`--state-dir` given to uninstall (or the `state_dir` of the config it reads).

## What is removed

A directory is removed with all its contents only when it clearly belongs to
madmail:

- **Config directory from a unit** (parent of `--config`): removed as a whole
  when its name is `madmail`, `chatmail`, `madmail-*`, `madmail_*`, `chatmail-*`,
  `chatmail_*` or the name of the running binary (for example
  `--config-dir /var/xyzzy/etc/madmail`). Any other directory, such as
  `--config-dir /usr/local/etc`, is shared: only the config file install wrote
  there (`<binary>.conf`) is removed. TLS files in its `certs/` subdirectory are
  left in place, because the unit does not show whether install created them; the
  plan names them so you can remove them yourself.
- **State directory from a unit** (`WorkingDirectory=`): removed as a whole under
  the same naming rule. A shared state directory is left in place, since install's
  files are mixed with the service's mail data there. To remove it with all its
  contents, pass it as `--state-dir` to uninstall.
- **Discovered directories** (`/etc/madmail*`, `/var/lib/madmail*`, …) and the
  `--state-dir` given to uninstall are removed as a whole.

Some directories are never removed as a whole, whatever their source: `/`,
`/etc`, `/usr`, `/usr/local`, `/usr/local/etc`, `/opt`, `/srv`, `/var`,
`/var/lib`, `/var/log`, `/var/cache`, `/var/mail`, `/home`, `/root`, `/tmp` and
other top-level system directories, every `/home/<name>`, and the home of root,
of each regular account (uid 1000 or more) in `/etc/passwd`, of `$HOME` and of the
`sudo` caller. The service accounts being removed are excluded, because their
home is the state directory. Both the path as written and its resolved form are
checked. For a unit config file in such a directory (for example
`/etc/madmail.conf`), only that file is removed.

`/run/<name>`, `/var/log/<name>` and `/var/cache/<name>` are removed for unit names
and for the names of removed state directories that follow the naming rule above.
Service accounts are deleted with `userdel` without `-r`: the home is the state
directory, and is removed (or kept with `--keep-data`) by the rules above.

The plan printed before confirmation lists each directory removed with all its
contents, each single file removed, and each path left in place with the reason.
The paths left in place are printed again when uninstall finishes. If the unit
file was removed or edited by hand, custom config and state directories outside
the default locations are not found; pass `--state-dir` and remove the config
directory yourself. Check the plan with `--dry-run` first.

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
