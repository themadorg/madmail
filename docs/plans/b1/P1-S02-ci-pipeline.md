# P1-S02: CI Pipeline

## Action

Add GitHub Actions workflow: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.

## Files touched

- `.github/workflows/ci.yml`

## TDD references

- [16-testing.md](../../TDD/16-testing.md) — CI and test strategy

## Madmail / context references

- `context/madmail/.github/workflows/` (if present) — prior CI patterns

## RFC references

_None._

## Verification

Push branch; workflow is green.

## Linked tests

All P1-UT* and P1-IT01 run in CI.
