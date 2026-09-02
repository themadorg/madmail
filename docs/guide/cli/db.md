# `db`

Copy the application SQLite database into PostgreSQL.


## Synopsis

```bash
madmail db sqlite-to-postgres --dsn DSN [OPTIONS]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `sqlite-to-postgres` | Copy application tables from SQLite into Postgres |

This copies **SQL rows only** (accounts, settings, quotas, tokens, federation, …). Maildir, `sharing.db`, the retry queue, TLS material, and `admin_token` stay on disk. Stop `madmail` before a live copy. Afterward, set `driver postgres` and restart — see [PostgreSQL operator guide](../../project/user-guide/18-postgres.md).

## Examples

```bash
madmail db sqlite-to-postgres --dsn 'postgres://madmail@127.0.0.1/madmail' --dry-run
sudo systemctl stop madmail
madmail db sqlite-to-postgres --dsn 'host=127.0.0.1 user=madmail dbname=madmail sslmode=disable' -y
```

## Subcommand pages

- [`sqlite-to-postgres`](db-sqlite-to-postgres.md) — `madmail db sqlite-to-postgres`

## JSON output (`--json`)

```bash
madmail db sqlite-to-postgres --dsn 'postgres://…' --dry-run --json
```

Schema: [json-output.md](json-output.md#db-sqlite-to-postgres).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/db.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/db.rs)
