# `tasks`

Run scheduled maintenance jobs on demand (retention, purge, certificate renewal).


## Synopsis

```bash
madmail tasks <list|run|run-all>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `list` | List jobs and config-driven schedule |
| `run <TASK> [--retention DURATION]` | Run one job now |
| `run-all` | Run all jobs enabled by config retention settings |

## Task names

| Name | Aliases | Description |
|------|---------|-------------|
| `prune-old-messages` | `prune-messages`, `retention` | Delete messages older than retention |
| `prune-unused-accounts` | `prune-unused`, `unused-accounts` | Remove accounts with no recent login |
| `purge-seen` | `purge-read`, `auto-purge-seen` | Delete seen (`cur/`) messages |
| `prune-unread-older` | `purge-unread-older` | Delete old `new/` messages (`--retention` required without config) |
| `renew-certificate` | `renew-cert`, `cert-renew`, `certificate-renew` | Renew Let's Encrypt cert (`tls_mode autocert`) |

## Examples

```bash
madmail tasks list
madmail tasks run prune-old-messages
madmail tasks run prune-unread-older --retention 720h
madmail tasks run-all
```

## Subcommand pages

- [`list`](tasks-list.md) — `madmail tasks list`
- [`run`](tasks-run.md) — `madmail tasks run`
- [`run-all`](tasks-run-all.md) — `madmail tasks run-all`

## JSON output (`--json`)

```bash
madmail tasks --json
```

Success stdout:

```json
{"ok": true, "command": "tasks", "data": { ... }}
```

Schema: [json-output.md](json-output.md#tasks).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/tasks.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/tasks.rs)
