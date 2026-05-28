# P4-S02: EnforceEncryption port

## Action

Parse RFC5322 stream.

## Files touched

_See [README](README.md)._

## TDD references

- [02-smtp-server.md](../../TDD/02-smtp-server.md)
- [12-security.md](../../TDD/12-security.md)

## Madmail / context references

- `context/madmail/internal/pgp_verify/pgp_verify.go`

## RFC references

- Index: [RFC library](../../TDD/RFC/README.md)

## Verification

```bash
cargo test -p chatmail-pgp
```

## Linked tests

| Test ID | Step |
|---------|------|
| _build-only_ | P4-S02 |

## Next

_See README._
