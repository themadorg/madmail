# P9-S07: Smoke — TURN Allocate with REST credentials

## Action

Add smoke test `turn_smoke_turn_allocate`:

1. Fixed `secret`, `realm`, generate `turn_metadata_line` → parse username/password.
2. Start turn-rs with `static-auth-secret = secret`.
3. Run STUN Allocate handshake ([RFC 8656](https://datatracker.ietf.org/doc/html/rfc8656) §6) with long-term credentials:
   - 401 + REALM + NONCE → retry with MESSAGE-INTEGRITY
4. Assert relay address in response.

Optional: if `COTURN_UCLIENT_PATH` set, shell out like `context/turn-rs/tests/turn.rs`.

## Files touched

- `crates/chatmail-turn/tests/smoke_allocate.rs`

## TDD references

- [11-proxy-services.md](../../TDD/11-proxy-services.md)

## Madmail / context references

- `context/turn-rs/tests/turn.rs` — coturn uclient integration

## RFC references

- [RFC 8656](../../TDD/RFC/rfc8656.txt) — Allocate, long-term credentials
- [draft-uberti-behave-turn-rest-00](../../TDD/RFC/draft-uberti-behave-turn-rest-00.txt) — password derivation

## Verification

**P9-SM02**

```bash
cargo test -p chatmail-turn turn_smoke_allocate
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P9-SM02** | `P9-S07` |

## Next

[P9-S08-integration-imap-turn.md](P9-S08-integration-imap-turn.md)
