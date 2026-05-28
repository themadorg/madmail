# P1-S09: Settings CRUD (DAO)

## Action

Implement `get_setting`, `set_setting` (upsert), `delete_setting` in `settings.rs` using parameterized SQL.

## Files touched

- `crates/chatmail-db/src/settings.rs`

## TDD references

- [09-admin-api.md](../../TDD/09-admin-api.md) — dynamic `__*__` settings
- [13-configuration.md](../../TDD/13-configuration.md) *(planned)*

## Madmail / context references

- `context/madmail/internal/table/sql_table.go`
- `context/madmail/docs/chatmail/settings_db.md`

## RFC references

- [RFC 8259](../../TDD/RFC/rfc8259.txt) — JSON admin API (Phase 9; settings values are strings today)

## Verification

**P1-UT04** `test_settings_crud`, **P1-UT06** `test_sql_injection_protection`.

```bash
cargo test -p chatmail-db test_settings
```
