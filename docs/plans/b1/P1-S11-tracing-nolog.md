# P1-S11: Tracing & No-Log Policy Setup

## Action

Add `logging.rs` with reloadable `EnvFilter`. When `log off` (default) and `debug` is false in config, set filter to `off`.

## Files touched

- `crates/chatmail/Cargo.toml`
- `crates/chatmail/src/logging.rs`

## TDD references

- [12-security.md](../../TDD/12-security.md) — No-Log policy (§2)
- [01-architecture.md](../../TDD/01-architecture.md) — observability

## Madmail / context references

- `context/madmail/docs/chatmail/nolog.md`
- `context/madmail/framework/log/`

## RFC references

_None._

## Verification

**P1-UT08** `test_dynamic_log_reload`.

```bash
cargo test -p chatmail test_dynamic_log_reload
```
