# P1-S10: Default Settings Resolution

## Action

Implement `get_bool_setting(pool, key, default)` — missing keys return `default`; accept `"true"` / `"false"` strings.

## Files touched

- `crates/chatmail-db/src/settings.rs`

## TDD references

- [12-security.md](../../TDD/12-security.md) — No-Log policy (`__LOG_DISABLED__`)
- [09-admin-api.md](../../TDD/09-admin-api.md) — service toggles

## Madmail / context references

- `context/madmail/docs/chatmail/nolog.md`
- `context/madmail/docs/chatmail/settings_db.md` — toggle table

## RFC references

_None._

## Verification

**P1-UT05** `test_bool_setting_defaults`.

```bash
cargo test -p chatmail-db test_bool_setting_defaults
```
