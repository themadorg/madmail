# P4-S03: multipart/encrypted check

## Action

Require valid PGP MIME.

## Files touched

_See [README](README.md)._

## TDD references

- [02-smtp-server.md](../../TDD/02-smtp-server.md)

## Madmail / context references

_See TDD implementation references._

## RFC references

RFC 3156: `RFC/rfc3156.txt`

## Verification

```bash
cargo test -p chatmail-pgp p4_ut02
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P4-UT02** | `P4-S03` |

## Next

_See README._
