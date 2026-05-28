# P2-S07: QuotaCache struct

## Action

`DashMap<String, QuotaEntry>` with used/max bytes.

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
cargo test -p chatmail-state quota
```

## Linked tests

| Test ID | Step |
|---------|------|
| _build-only_ | P2-S07 |

## Next

_See README._
