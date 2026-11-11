# `legacy-migrate`

Point an existing `maddy` command and systemd unit at the active `madmail` binary.

`upgrade` and `update` do this themselves. This command is the one-time entry for a host whose older `maddy upgrade` already stored the new release at `/usr/local/bin/madmail` and left `maddy.service` executing the previous file.

## Synopsis

```bash
sudo madmail legacy-migrate
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |
| `--json` | — | — | off | Emit machine-readable JSON on stdout (no decorative text or QR codes) |

## What it changes

The command reads `maddy.service` / `madmail.service` and drop-ins, then:

- Rewrites the program path to `/usr/local/bin/madmail`. `--config`, the state directory, `User`, environment, and the rest of `ExecStart` stay byte for byte.
- When the effective unit file lives under `/lib` or `/usr/lib`, writes `/etc/systemd/system/<unit>.d/madmail-exec.conf` (`ExecStart=` to clear the list, then the new line). An existing unit under `/etc` is edited in place when it is a regular file.
- When `/usr/local/bin/maddy` already exists and is this project, replaces it with a symlink to `/usr/local/bin/madmail`. A regular file is kept beside it as `maddy.pre-migrate`.

Config files, credentials, mailboxes, keys, and tokens are left where they are. The command does not regenerate a config.

## When it refuses

Nothing is written when:

- Two Madmail units name different `--config`, `--state-dir`, or `--libexec` values.
- Two legacy units start the same install from the old executable.
- `ExecStart` is more than one command, or a line continues with a trailing backslash.

An unrelated `maddy` (for example another mail server whose `version` output does not contain `madmail-v2`) is ignored.

## Already in place

Running the command again when the unit and the `maddy` command already use `/usr/local/bin/madmail`, and the running process is that binary, prints that fact and does not rewrite or restart.

When the files are already canonical and the running process is still the old file, the command restarts the unit and checks `/proc/<pid>/exe`. It does not rewrite the unit.

## If the stable binary is missing

An older `maddy upgrade` is what installs `/usr/local/bin/madmail` and then leaves the service on the old file. From the new release archive, run that archive's own binary:

```bash
sudo ./madmail upgrade <signed-file-or-url>
```

The 2.26.0 `maddy upgrade` command cannot switch the service. After the new binary is at `/usr/local/bin/madmail`, `sudo madmail legacy-migrate` finishes a host that is still on the old unit.

## Examples

```bash
sudo madmail legacy-migrate

sudo madmail legacy-migrate --json
```

## Notes

- Linux with systemd. On other systems the command exits with an error explaining that.
- Requires root (`sudo`).
- After a unit edit, success means the service main process is the activated binary. A `version` check of the new file alone is not enough. If the process is still the old file, the unit and the `maddy` command are restored and the command exits with an error.
- A vendor unit is never edited in place. The drop-in under `/etc/systemd/system` wins.

## JSON output (`--json`)

```bash
madmail legacy-migrate --json
```

Success stdout:

```json
{"ok": true, "command": "legacy-migrate", "message": "Legacy maddy service now runs the active madmail binary", "data": { "changed": true }}
```

Schema: [json-output.md](json-output.md#legacy-migrate).

---
[← CLI index](README.md) · [Global flags](global-flags.md) · [`upgrade`](upgrade.md)

[Source: `crates/chatmail/src/ctl/legacy_migrate.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/legacy_migrate.rs)
