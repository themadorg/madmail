# `sharing`

Manage Delta Chat contact share links stored in `sharing.db`.


## Synopsis

```bash
madmail sharing <status|enable|disable|list|create|reserve|remove|edit>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show whether public contact pages are enabled |
| `enable` | Turn public contact pages on. Saved links stay in place |
| `disable` | Turn public contact pages off without deleting saved links |
| `list` | List all share links |
| `create <SLUG> <URL> [NAME]` | Create a link |
| `reserve <SLUG>` | Reserve slug (points to `reserved`) |
| `remove <SLUG>` | Remove link (alias: `delete`) |
| `edit <SLUG> <NEW_URL> [NEW_NAME]` | Update link |

## Examples

```bash
madmail sharing status
madmail sharing disable
madmail sharing enable
madmail sharing list
madmail sharing create alice https://example.org/alice.vcf "Alice"
madmail sharing reserve bob
madmail sharing edit alice https://example.org/new.vcf
madmail sharing remove bob
```

## Notes

`status`, `enable`, and `disable` share one saved setting, `__CONTACT_SHARING_ENABLED__`. A saved value overrides `enable_contact_sharing` in the config file (`enable_contact_sharing true` inside the maddy `chatmail` block, or `enable_contact_sharing = true` in TOML). With no saved value, the config flag is the effective one. The default config value is off.

`enable` and `disable` write the database row. That applies on the next request to `/share` or a public contact page. No service restart and no `madmail reload` are required. `status` then reports `source` `database`, `reload_required` false, and `file_change_requires_restart` false.

With no saved row, `status` reports `source` `config` and `file_change_requires_restart` true. The running server keeps the file value from when it started. Editing `enable_contact_sharing` needs a restart. `madmail reload` does not reread that directive. `reload_required` stays false because reload is not the step that applies the file.

Disabling returns 404 for `GET` and `POST /share` and for existing public slug pages. `sharing.db` and its rows stay. `list`, `create`, `reserve`, `edit`, and `remove` keep working while public pages are off. Enabling again serves the saved links.

`enable` and `disable` do not print saved URLs.

## Subcommand pages

- [`status`](sharing-status.md) — `madmail sharing status`
- [`enable`](sharing-enable.md) — `madmail sharing enable`
- [`disable`](sharing-disable.md) — `madmail sharing disable`
- [`create`](sharing-create.md) — `madmail sharing create`
- [`edit`](sharing-edit.md) — `madmail sharing edit`
- [`list`](sharing-list.md) — `madmail sharing list`
- [`remove`](sharing-remove.md) — `madmail sharing remove`
- [`reserve`](sharing-reserve.md) — `madmail sharing reserve`

## JSON output (`--json`)

```bash
madmail sharing --json
```

Success stdout:

```json
{"ok": true, "command": "sharing", "data": { ... }}
```

Schema: [json-output.md](json-output.md#sharing).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/sharing.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/sharing.rs)
