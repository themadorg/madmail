# `endpoint-cache`

Manage outbound delivery endpoint overrides (DNS routing cache). Alias: [`dns-cache`](dns-cache.md).


## Synopsis

```bash
madmail endpoint-cache <list|set|get|remove>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `list` | List all override entries |
| `set <LOOKUP_KEY> <TARGET_HOST> [COMMENT]` | Create or update an entry |
| `get <LOOKUP_KEY>` | Show one entry |
| `remove <LOOKUP_KEY>` | Remove an entry (alias: `delete`) |

## Examples

```bash
madmail endpoint-cache list
madmail endpoint-cache set mail.partner.com smtp.partner.com "Route via partner"
madmail endpoint-cache get mail.partner.com
madmail endpoint-cache remove mail.partner.com
```

Use for advanced routing when delivering to specific remote hosts.

## Subcommand pages

- [`get`](endpoint-cache-get.md) — `madmail endpoint-cache get`
- [`list`](endpoint-cache-list.md) — `madmail endpoint-cache list`
- [`remove`](endpoint-cache-remove.md) — `madmail endpoint-cache remove`
- [`set`](endpoint-cache-set.md) — `madmail endpoint-cache set`

## JSON output (`--json`)

```bash
madmail endpoint cache --json
```

Success stdout:

```json
{"ok": true, "command": "endpoint cache", "data": { ... }}
```

Schema: [json-output.md](json-output.md#endpoint-cache).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/endpoint_cache.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/endpoint_cache.rs)
