# P1-S07: Base Schema Migrations

## Action

Create `migrations/20240101000000_init.sql` with tables: `settings`, `quotas`, `blocked_users`, `registration_tokens`, `dns_overrides`.

## Files touched

- `crates/chatmail-db/migrations/20240101000000_init.sql`
- `crates/chatmail-db/src/models.rs`

## TDD references

- [17-data-models.md](../../TDD/17-data-models.md) *(planned)*
- [09-admin-api.md](../../TDD/09-admin-api.md) — settings keys consumed by admin API

## Madmail / context references

- `context/madmail/docs/chatmail/settings_db.md` — table definitions
- `context/madmail/internal/db/models.go`

## RFC references

_None._

## Verification

```bash
sqlite3 /tmp/x.db < crates/chatmail-db/migrations/20240101000000_init.sql
.schema
```
