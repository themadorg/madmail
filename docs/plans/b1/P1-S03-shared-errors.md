# P1-S03: Shared Error Types

## Action

Initialize `chatmail-types` with `thiserror`. Define `ChatmailError`: `Db(#[from] sqlx::Error)`, `Config(String)`, `Io(#[from] std::io::Error)`.

## Files touched

- `crates/chatmail-types/Cargo.toml`
- `crates/chatmail-types/src/error.rs`
- `crates/chatmail-types/src/lib.rs`

## TDD references

- [01-architecture.md](../../TDD/01-architecture.md) — error propagation across crates

## Madmail / context references

- `context/madmail/framework/exterrors/` — shared error types

## RFC references

_None._

## Verification

```bash
cargo check -p chatmail-types
```

## Linked tests

_None._
