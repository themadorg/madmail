# P2-S03: Atomic write_blob

## Action

Write to `tmp/`, `sync_data`, rename to `new/`.

## Files touched

_See [README](README.md)._

## TDD references

- [04-storage-layer.md](../../TDD/04-storage-layer.md)

## Madmail / context references

_See TDD implementation references._

## RFC references

RFC 5322 message blobs: `RFC/rfc5322.txt`

## Verification

```bash
cargo test -p chatmail-storage p2_ut02
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P2-UT02** | `P2-S03` |

## Next

_See README._
