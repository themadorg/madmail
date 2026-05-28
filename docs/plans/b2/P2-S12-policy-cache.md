# P2-S12: FederationPolicyCache

## Action

ACCEPT blocklist / REJECT allowlist via `RwLock<HashSet>`.

## Files touched

_See [README](README.md)._

## TDD references

- [07-federation.md](../../TDD/07-federation.md)
- [12-security.md](../../TDD/12-security.md)

## Madmail / context references

_See TDD implementation references._

## RFC references

- Index: [RFC library](../../TDD/RFC/README.md)

## Verification

```bash
cargo test -p chatmail-state p2_ut05
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P2-UT05** | `P2-S12` |

## Next

_See README._
