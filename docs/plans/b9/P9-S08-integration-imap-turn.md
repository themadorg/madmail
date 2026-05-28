# P9-S08: Integration — IMAP metadata authenticates on TURN

## Action

Add `tests/turn_e2e.rs` in `chatmail-integration`:

1. Extend `spawn_mail_servers` in [`tests/support/mod.rs`](../../../tests/support/mod.rs):
   - Bind ephemeral TURN port
   - Set `turn_enable`, `turn_secret`, `turn_server=127.0.0.1`
   - Start embedded turn-rs
2. Test `turn_metadata_auth`:
   - `ImapClient`: LOGIN → `GETMETADATA "" /shared/vendor/deltachat/turn`
   - Parse line; run minimal TURN Allocate with returned creds against local turn-rs
   - Assert success (proves IMAP + TURN secret alignment)

No Delta Chat binary required at this tier.

## Files touched

- `tests/support/mod.rs` — `MailServers { turn_addr, turn_secret }`
- `tests/turn_e2e.rs` (new)
- `tests/Cargo.toml` — `[[test]] name = "turn_e2e"`

## TDD references

- [11-proxy-services.md](../../TDD/11-proxy-services.md) — integration tier
- [16-testing.md](../../TDD/16-testing.md)

## Madmail / context references

- `tests/imap_e2e.rs` — `spawn_mail_servers` pattern

## RFC references

- [RFC 5464](../../TDD/RFC/rfc5464.txt)
- [RFC 8656](../../TDD/RFC/rfc8656.txt)

## Verification

**P9-IT01**

```bash
cargo test -p chatmail-integration turn_metadata_auth
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P9-IT01** | `P9-S08` |

## Next

[P9-S09-e2e-relay-ping-imap.md](P9-S09-e2e-relay-ping-imap.md)
