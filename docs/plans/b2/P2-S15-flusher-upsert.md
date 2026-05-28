# P2-S15: UPSERT federation_server_stats

## Action

Batch upsert tracker snapshot to SQLite.

## Files touched

_See [README](README.md)._

## TDD references

- [07-federation.md](../../TDD/07-federation.md)

## Madmail / context references

_See TDD implementation references._

## RFC references

- Index: [RFC library](../../TDD/RFC/README.md)

## Verification

```bash
cargo test -p chatmail-state p2_ut06
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P2-UT06** | `P2-S15` |

## Next

_See README._
