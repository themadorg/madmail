# `madmail db sqlite-to-postgres`

Parent: [`db`](db.md)

Copy application tables from the SQLite file into PostgreSQL. Does **not** rewrite `madmail.conf`; you still set `driver postgres` and restart.

## Synopsis

```bash
madmail db sqlite-to-postgres --dsn <DSN> [--sqlite PATH] [--dry-run] [--force] [-y]
```

## Options

| Option | Description |
|--------|-------------|
| `--dsn` | Postgres URL (`postgres://…`) or libpq `key=value` string |
| `--sqlite` | SQLite file (default: application DB from config / state-dir) |
| `--dry-run` | Count SQLite rows only; do not connect to Postgres |
| `--force` | Replace existing Postgres `passwords` rows (otherwise refuse if non-empty) |
| `-y`, `--yes` | Skip confirmation (`--dry-run` does not prompt) |

## Notes

- Stop the server before copying a live database.
- `--dry-run` never writes. A real copy creates the v2 Postgres schema (sqlx migrations) then inserts rows.
- Mail files are not copied. `sharing.db` is not copied.

## JSON output (`--json`)

```bash
madmail db sqlite-to-postgres --dsn 'postgres://madmail@127.0.0.1/madmail' --dry-run --json
```

Schema: [json-output.md](json-output.md#db-sqlite-to-postgres).

---
[← `db`](db.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/db.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/db.rs)
