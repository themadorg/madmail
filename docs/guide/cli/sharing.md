# `sharing`

Manage Delta Chat contact share links stored in `sharing.db`.


## Synopsis

```bash
madmail sharing <list|create|reserve|remove|edit>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `list` | List all share links |
| `create <SLUG> <URL> [NAME]` | Create a link |
| `reserve <SLUG>` | Reserve slug (points to `reserved`) |
| `remove <SLUG>` | Remove link (alias: `delete`) |
| `edit <SLUG> <NEW_URL> [NEW_NAME]` | Update link |

## Examples

```bash
madmail sharing list
madmail sharing create alice https://example.org/alice.vcf "Alice"
madmail sharing reserve bob
madmail sharing edit alice https://example.org/new.vcf
madmail sharing remove bob
```

## Subcommand pages

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
