# P2-S09: check_quota

## Action

Reject when `used + incoming > max`.

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
cargo test -p chatmail-state p2_ut03
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P2-UT03** | `P2-S09` |

## Next

_See README._
