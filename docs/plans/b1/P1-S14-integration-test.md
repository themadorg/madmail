# P1-S14: Integration Smoke Test

## Action

Add workspace `tests/boot_test.rs`: spawn `chatmail` with temp `--state-dir`, assert exit 0, `chatmail.db` and `admin_token` exist.

## Files touched

- `tests/Cargo.toml`
- `tests/boot_test.rs`
- `Cargo.toml` (workspace member `tests`)

## TDD references

- [16-testing.md](../../TDD/16-testing.md) — integration smoke tests

## Madmail / context references

- `context/madmail/tests/deltachat-test/` — future E2E harness (Phase 4+)

## RFC references

_None._

## Verification

**P1-IT01** `test_binary_boots_and_migrates`.

```bash
cargo test -p chatmail-integration boot_test
```
