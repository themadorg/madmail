# `delete`

Fully delete a user: credentials, mail storage, and blocklist entry.


## Synopsis

```bash
madmail delete <USERNAME> [-y] [--reason REASON]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Arguments & options

| Argument / flag | Description |
|-----------------|-------------|
| `USERNAME` | Account email or local username |
| `-y`, `--yes` | Skip confirmation prompt |
| `--reason` | Blocklist reason stored in DB (default: `account deleted via CLI`) |

## Example

```bash
madmail delete gone@example.org --yes --reason "left the team"
```

## Related

- [`accounts delete`](accounts-delete.md) — same removal without `--reason` (uses default blocklist reason)
- [`accounts ban`](accounts-ban.md) — delete with explicit moderation reason positional argument

## JSON output (`--json`)

```bash
madmail delete --json
```

Success stdout:

```json
{"ok": true, "command": "delete", "data": { ... }}
```

Schema: [json-output.md](json-output.md#delete).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/delete_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/delete_cmd.rs)
