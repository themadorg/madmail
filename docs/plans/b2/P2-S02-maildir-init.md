# P2-S02: MailboxStore + init_user_dir

## Action

Create `{state_dir}/mail/{{user}}/Maildir/{{cur,new,tmp}}`.

## Files touched

_See [README](README.md)._

## TDD references

- [04-storage-layer.md](../../TDD/04-storage-layer.md)

## Madmail / context references

- `context/madmail — Maildir layout`

## RFC references

- Index: [RFC library](../../TDD/RFC/README.md)

## Verification

```bash
cargo test -p chatmail-storage p2_ut01
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P2-UT01** | `P2-S02` |

## Next

_See README._
