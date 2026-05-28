# P2-S08: QuotaCache::hydrate

## Action

Load `quotas` table; sum maildir sizes per user.

## Files touched

_See [README](README.md)._

## TDD references

- [04-storage-layer.md](../../TDD/04-storage-layer.md)

## Madmail / context references

- `context/madmail/internal/storage/imapsql/imapsql.go `populateQuotaCache``

## RFC references

- Index: [RFC library](../../TDD/RFC/README.md)

## Verification

```bash
cargo test -p chatmail-state hydrate
```

## Linked tests

| Test ID | Step |
|---------|------|
| _build-only_ | P2-S08 |

## Next

_See README._
