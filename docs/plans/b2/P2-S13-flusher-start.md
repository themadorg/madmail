# P2-S13: start_flusher

## Action

Spawn background task with shutdown channel.

## Files touched

_See [README](README.md)._

## TDD references

- [07-federation.md](../../TDD/07-federation.md)
- [16-testing.md](../../TDD/16-testing.md)

## Madmail / context references

- `tracker.go `StartFlusher``

## RFC references

- Index: [RFC library](../../TDD/RFC/README.md)

## Verification

```bash
cargo test -p chatmail-state flusher
```

## Linked tests

| Test ID | Step |
|---------|------|
| _build-only_ | P2-S13 |

## Next

_See README._
