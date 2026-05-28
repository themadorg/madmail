# P1-S06: SQLite Database Setup

## Action

Initialize `chatmail-db` with `sqlx` (`sqlite`, `runtime-tokio`, `migrate`, `macros`). Implement `pub async fn init_db(db_path: &Path) -> Result<SqlitePool, ChatmailError>`.

## Files touched

- `crates/chatmail-db/Cargo.toml`
- `crates/chatmail-db/src/lib.rs`

## TDD references

- [04-storage-layer.md](../../TDD/04-storage-layer.md) — settings DB vs mail storage split
- [17-data-models.md](../../TDD/17-data-models.md) *(planned)* — full schema

## Madmail / context references

- `context/madmail/internal/db/gormsqlite/dialector.go` — SQLite connection options
- `context/madmail/docs/internals/database.md`

## RFC references

_None (local SQLite)._

## Verification

```bash
cargo check -p chatmail-db
```
