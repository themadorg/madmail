# `accounts`

Direct database account management (credentials, maildirs, quota, blocklist).


## Synopsis

```bash
madmail accounts <subcommand>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Summary of credentials and storage |
| `info <username>` | One account: credentials, quota, blocklist status |
| `create <username> [--password PASS]` | Create login + maildir + quota row; prints a `dclogin:` URI |
| `dclogin <username> [--password PASS]` | Reprint a `dclogin:` URI for an existing account |
| `create-random [--json-only]` | Random username/password; prints JSON with `dclogin` link |
| `delete <username> [-y]` | Remove credentials, mail, and blocklist entry |
| `ban <username> [reason] [-y]` | Same as delete with moderation reason |
| `unban <username> [-y]` | Remove blocklist only (does not restore mail) |
| `ban-list` | List blocklisted usernames |
| `export [-o FILE]` | Export usernames (and hashes) as JSON |
| `import <file>` | Import from JSON array `[{username, password?, hash?}]` |
| `delete-all [-y]` | Delete **all** user accounts (destructive) |

## Examples

```bash
madmail accounts status
madmail accounts info alice@example.org
madmail accounts create bob@example.org --password 'secret'
madmail accounts dclogin bob@example.org --password 'secret'
madmail accounts ban spammer@example.org "spam" --yes
madmail accounts export -o accounts-backup.json
madmail accounts import accounts-backup.json
```

## Notes

- Usernames without `@` are expanded using the server's registration domain from config.
- `ban` / `delete` remove mail data permanently.

## Subcommand pages

- [`ban`](accounts-ban.md) — `madmail accounts ban`
- [`ban-list`](accounts-ban-list.md) — `madmail accounts ban-list`
- [`create`](accounts-create.md) — `madmail accounts create`
- [`create-random`](accounts-create-random.md) — `madmail accounts create-random`
- [`dclogin`](accounts-dclogin.md) — `madmail accounts dclogin`
- [`delete`](accounts-delete.md) — `madmail accounts delete`
- [`delete-all`](accounts-delete-all.md) — `madmail accounts delete-all`
- [`export`](accounts-export.md) — `madmail accounts export`
- [`import`](accounts-import.md) — `madmail accounts import`
- [`info`](accounts-info.md) — `madmail accounts info`
- [`status`](accounts-status.md) — `madmail accounts status`
- [`unban`](accounts-unban.md) — `madmail accounts unban`

## JSON output (`--json`)

```bash
madmail accounts --json
```

Success stdout:

```json
{"ok": true, "command": "accounts", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
