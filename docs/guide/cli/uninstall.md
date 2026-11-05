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
state directory in use: the `state_dir` of the config file uninstall reads
(`--config`) when it sets one, otherwise `--state-dir`.

`state_dir` from the config file is used only when these all hold:

- the config file, and every directory and link on its path, is owned by root;
- the config file has one hard link, and is not group-writable unless its group
  is root;
- no directory on the path is group-writable by a group of the service. These
  groups are the primary and supplementary groups of the service accounts (the
  `User=` accounts of the units, the accounts named after the units, and the
  accounts named like the binary, `madmail` or `chatmail`; the same accounts
  count when uninstall checks the way to a path it removes), the group that
  owns the config file's directory, and
  the group that owns the state directory (the one from the config file and the
  one from `--state-dir`). When one of these accounts exists but its groups
  cannot be read, the config file is not used;
- no directory on the path is group-writable by any other group except root,
  unless it has the sticky bit. Only `/var/log` and `/var/mail` (matched by
  device and inode) may be group-writable by another system group, such as
  `/var/log` on Ubuntu (`root:syslog`, mode 0775), and only when that group is
  not a group of the service;
- nothing on the path is world-writable (a directory with the sticky bit, such
  as `/tmp`, may be).

Mode bits are read, not ACLs. Otherwise uninstall uses `--state-dir`, and lists the
ignored directory as left in place with the reason
`named by state_dir in <config>, which is not used: <component> is <reason>`
(unless another source plans that directory for removal). A `--dry-run` without
root also accepts the caller's own files and directories in this check.

## What is removed

The same rule applies to every config and state directory, whatever found it
(a unit, `/etc` or `/var/lib` discovery, `--state-dir`, or the config file). A
directory is removed with all its contents only when its name is `madmail`,
`chatmail`, `madmail-*`, `madmail_*`, `chatmail-*`, `chatmail_*` or the name of
the running binary (for example `/var/xyzzy/etc/madmail`), and it is not one of
the directories listed below that are never removed as a whole.

- **Other config directories**, such as `--config-dir /usr/local/etc`: only the
  config file install wrote there (`<binary>.conf`) is removed. TLS files in its
  `certs/` subdirectory are left in place, because nothing shows whether install
  created them; the plan names them so you can remove them yourself. A `--config`
  file with another name is not removed.
- **Other state directories**, such as `/srv/data`: left in place, also when
  given with `--state-dir`, since the service's mail data may be mixed with other
  files there. Remove it yourself.

Some directories are never removed as a whole, whatever their source: `/`,
`/etc`, `/usr`, `/usr/local`, `/usr/local/etc`, `/opt`, `/srv`, `/var`,
`/var/lib`, `/var/log`, `/var/cache`, `/var/mail`, `/home`, `/root`, `/tmp` and
other system directories, every directory directly under `/` (such as `/data` or
`/madmail`), every directory below `/etc`, `/usr`, `/lib`, `/lib32`, `/lib64`,
`/libx32`, `/bin`, `/sbin`, `/boot`, `/dev`, `/proc`, `/sys` or `/run` none of
whose components below the top-level one is named for madmail (such as
`/etc/systemd`; `/etc/madmail` and `/etc/madmail/data` are named for it), every
`/home/<name>`, and the home of root,
of each regular account (uid 1000 or more) in `/etc/passwd`, of `$HOME` and of the
`sudo` caller. The service accounts being removed are excluded, because their
home is the state directory. Both the path as written and its resolved form are
checked. For a config file in such a directory (for example `/etc/madmail.conf`),
only that file is removed.

A config directory or file inside a state directory that is removed goes with
that directory and gets no removal of its own. Under `--keep-data` it is left in
place with the reason
`inside state directory <path>, which --keep-data keeps`. Only a path that is
inside the state directory both as written and with links resolved counts.

`--keep-data` and `--keep-config` always hold. A directory that is, or contains,
a path that stays (a path left in place, a state directory under `--keep-data`,
or a config directory or file under `--keep-config`) is not removed as a whole: a
config directory then loses only its config file, and a state, log, cache or
runtime directory is left in place. This is checked both on the paths as written
and after links in them are resolved, so a config directory that is a link into
the state directory counts as being inside it. A path that cannot be resolved
counts as inside.

Before each removal, every directory and link on the way to the path must pass
the same check as the config file above. When one does not, nothing is removed
at that path, the output prints
`⚠️  Left in place: <path> (<component> is <reason>; check it and remove the path yourself)`,
and uninstall goes on with the next path. A removal that fails for another
reason (for example a file that cannot be unlinked or a mount point) is also
left in place, as
`⚠️  Left in place: <path> (removing it failed: <error>; remove it yourself)`,
and uninstall goes on. The path is listed again with the
other paths left in place when uninstall finishes. When the groups of a service
account cannot be read, uninstall prints
`⚠️  <why>; no directory writable by a group other than root is trusted on the way to a path to remove`:
the groups of the service are then only the groups that own the state
directories, and the `/var/log` and `/var/mail` allowance in the check is turned
off, so a directory on the way that is writable by a group other than root,
without the sticky bit, leaves the path in place. The last component is not
followed: a link is removed itself, not its target (the output says
`Removed link <path> (its target <target> was left in place)`), and a directory is
removed through descriptors from its checked parent down, without following
links inside it.

`/run/<name>`, `/var/log/<name>` and `/var/cache/<name>` are removed for unit names
and for the names of removed state directories that follow the naming rule above.
Service accounts are deleted with `userdel` without `-r`: the home is the state
directory, and is removed (or kept) by the rules above.

The plan printed before confirmation lists each directory removed with all its
contents, each single file removed, and each path left in place with the reason.
The paths left in place are printed again when uninstall finishes. If the unit
file was removed or edited by hand, custom config and state directories outside
the default locations are not found; pass `--state-dir` (it is removed only when
its name follows the rule above) and remove the config directory yourself. Check
the plan with `--dry-run` first.

## Reasons for paths left in place

The plan, the final output (`Left in place: <path> (<reason>)`) and the JSON
`left_in_place` list use these reasons:

| Reason | Meaning |
|--------|---------|
| `well-known system directory` | A directory from the deny list, such as `/var/lib`. |
| `top-level directory` | Any other direct child of `/`, such as `/data`. |
| `directory below /etc, /usr or another system directory` | A directory below `/etc`, `/usr`, `/lib`, `/bin`, `/sbin`, `/boot`, `/dev`, `/proc`, `/sys` or `/run` (and the other trees listed above) that is not named for madmail, such as `/etc/systemd`. |
| `user home directory` | `/home/<name>`, `/Users/<name>` or a real account's home. |
| `shared directory, not named for madmail; remove it yourself` | A state directory that does not follow the naming rule. |
| `<why>; only <binary>.conf is removed` | A config directory that is not removed as a whole; `<why>` is one of the two reasons above, `shared directory, not named for madmail`, or a `contains ...` reason below. |
| `<why>; <file> is not a config file name install writes` | A unit's `--config` file in such a directory, with another name. |
| `<why>; nothing in it is removed` | A config directory that contains a path that stays, where no config file install wrote is known. |
| `TLS file in a shared directory; install may not have created it, remove it yourself if it did` | `certs/fullchain.pem` or `certs/privkey.pem` in a config directory that is not removed as a whole. |
| `contains <path>, which is left in place; remove the rest yourself` | A state, log, cache or runtime directory that contains a path left in place. |
| `contains <path>, which --keep-config keeps; remove the rest yourself` | A state, log, cache or runtime directory that contains the config. |
| `named by state_dir in <config>, which is not used: <why>` | `state_dir` from a config file that failed the check above. |
| `inside state directory <path>, which --keep-data keeps` | A config directory or file inside a state directory kept by `--keep-data`. |
| `removing it failed: <error>; remove it yourself` | The removal itself failed, for example because a file could not be unlinked. Uninstall went on with the other paths. |
| `<component> is <reason>; check it and remove the path yourself` | A path whose way runs through a directory or link that failed the check above. It is listed whatever `--keep-config` and `--keep-data` say. |

For a config directory the `contains` reasons read
`contains <path>, which is left in place` or
`contains <path>, which --keep-data keeps`, followed by
`; only <binary>.conf is removed` or `; nothing in it is removed`.

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
