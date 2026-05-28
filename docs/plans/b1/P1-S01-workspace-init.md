# P1-S01: Workspace Initialization

## Action

Create the root `Cargo.toml` with `[workspace]` and members for all crates plus `tests/`. Pin shared versions under `[workspace.dependencies]` (`tokio`, `tracing`, `sqlx`, `serde`, `thiserror`, `toml`, `anyhow`).

## Files touched

- `Cargo.toml` (workspace root)
- `crates/*/Cargo.toml`

## TDD references

- [00-intro.md](../../TDD/00-intro.md) — project goals, Rust/Tokio stack
- [01-architecture.md](../../TDD/01-architecture.md) — crate boundaries and workspace layout

## Madmail / context references

- `context/madmail/go.mod` — dependency pinning analogue

## RFC references

_None for this step._

## Verification

```bash
cargo build --workspace
```

## Linked tests

_None (build-only step)._
