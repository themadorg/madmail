# P5-S10: APPEND + PGP gate

## Action

enforce_encryption on append.

## Files touched

_See [README](README.md)._

## TDD references

- [03-imap-server.md](../../TDD/03-imap-server.md)
- [12-security.md](../../TDD/12-security.md)

## Madmail / context references

_See TDD implementation references._

## RFC references

- Index: [RFC library](../../TDD/RFC/README.md)

## Verification

```bash
cargo test -p chatmail-imap p5_ut03
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P5-UT03** | `P5-S10` |

## Next

_See README._
