# `blocklist`

Manage usernames blocked from re-registration (separate from active account deletion).


## Synopsis

```bash
madmail blocklist <list|add|remove>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `list` | List all blocked users |
| `add <username> [reason]` | Block username (default reason: manually blocked via CLI) |
| `remove <username> [-y]` | Unblock username |

## Examples

```bash
madmail blocklist list
madmail blocklist add baduser@example.org spam
madmail blocklist remove baduser@example.org --yes
```

## Subcommand pages

- [`add`](blocklist-add.md) — `madmail blocklist add`
- [`list`](blocklist-list.md) — `madmail blocklist list`
- [`remove`](blocklist-remove.md) — `madmail blocklist remove`

## JSON output (`--json`)

```bash
madmail blocklist --json
```

Success stdout:

```json
{"ok": true, "command": "blocklist", "data": { ... }}
```

Schema: [json-output.md](json-output.md#blocklist).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/blocklist_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/blocklist_cmd.rs)
