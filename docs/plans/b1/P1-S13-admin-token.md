# P1-S13: Admin Token Generation

## Action

Extract `admin::ensure_admin_token` — create `{state_dir}/admin_token` (64 hex chars, mode `0600`) if missing; never overwrite valid token.

## Files touched

- `crates/chatmail/src/admin.rs`
- `crates/chatmail/src/main.rs`

## TDD references

- [09-admin-api.md](../../TDD/09-admin-api.md) — Bearer auth, token path
- [12-security.md](../../TDD/12-security.md) — admin API security (§5)

## Madmail / context references

- `context/madmail/docs/chatmail/admin_api.md`
- `context/madmail/internal/endpoint/chatmail/chatmail.go` — `ensureAdminToken`

## RFC references

- [RFC 6750](../../TDD/RFC/rfc6750.txt) — Bearer token usage (HTTP admin API, Phase 9)
- [RFC 9110](../../TDD/RFC/rfc9110.txt) — HTTP semantics (admin transport)

## Verification

**P1-UT07** `test_admin_token_generation`.

```bash
cargo test -p chatmail test_admin_token_generation
```
