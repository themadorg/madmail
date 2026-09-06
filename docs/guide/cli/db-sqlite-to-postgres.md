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
| `--force` | `DELETE` every row from all copied Postgres tables first (otherwise the copy is refused when any of them is non-empty) |
| `-y`, `--yes` | Skip confirmation (`--dry-run` does not prompt) |

## Notes

- Stop the server before copying a live database.
- `--dry-run` never writes. A real copy creates the v2 Postgres schema (sqlx migrations) then inserts rows.
- The copy runs in a single transaction, including the `--force` deletes: if any table fails, the Postgres database is left exactly as it was.
- Copying into a Postgres database whose `passwords` table still uses the legacy Madmail key/value layout is refused; migrate that database to the v2 schema first.
- Legacy Madmail (Go) databases are read from their own table and column names (singular `quota`, `failed_http_s` / `success_http_s`). The report's `source_table` column shows what was actually read.
- Mail files are not copied. `sharing.db` is not copied.

## JSON output (`--json`)

```bash
madmail db sqlite-to-postgres --dsn 'postgres://madmail@127.0.0.1/madmail' --dry-run --json
```

Schema: [json-output.md](json-output.md#db-sqlite-to-postgres).

---
[← `db`](db.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/db.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/db.rs)
