# P1-S08: Apply Migrations on Boot

## Action

In `init_db`, run `sqlx::migrate!("./migrations")`, enable `PRAGMA journal_mode=WAL` and `foreign_keys=ON`.

## Files touched

- `crates/chatmail-db/src/lib.rs`

## TDD references

- [16-testing.md](../../TDD/16-testing.md) — migration idempotency tests

## Madmail / context references

- `context/madmail/internal/db/gormsqlite/` — AutoMigrate analogue

## RFC references

_None._

## Verification

**P1-UT03** `test_db_migration_idempotency`.

```bash
cargo test -p chatmail-db test_db_migration_idempotency
```
