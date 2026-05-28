# P2-S14: 30s flush interval

## Action

`tokio::time::interval(Duration::from_secs(30))`.

## Files touched

_See [README](README.md)._

## TDD references

- [04-storage-layer.md](../../TDD/04-storage-layer.md)

## Madmail / context references

_See TDD implementation references._

## RFC references

- Index: [RFC library](../../TDD/RFC/README.md)

## Verification

```bash
RUST_LOG=debug cargo run …
```

## Linked tests

| Test ID | Step |
|---------|------|
| _build-only_ | P2-S14 |

## Next

_See README._
