# P9-S06: Smoke — STUN Binding

## Action

Add smoke test `turn_smoke_stun_binding`:

1. Start turn-rs on `127.0.0.1:0` (UDP).
2. Send STUN Binding Request ([RFC 8489](https://datatracker.ietf.org/doc/html/rfc8489) §6).
3. Assert success response with XOR-MAPPED-ADDRESS.

Reuse turn-rs codec types or minimal test client in `chatmail-turn/tests/`.

## Files touched

- `crates/chatmail-turn/tests/smoke_stun.rs`

## TDD references

- [11-proxy-services.md](../../TDD/11-proxy-services.md) — smoke tier

## Madmail / context references

- `context/turn-rs/tests/stun.rs` — message fixtures

## RFC references

- [RFC 8489](../../TDD/RFC/rfc8489.txt) — Binding method
- [RFC 5769](../../TDD/RFC/rfc5769.txt) — test vectors (optional cross-check)

## Verification

**P9-SM01**

```bash
cargo test -p chatmail-turn turn_smoke_stun
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P9-SM01** | `P9-S06` |

## Next

[P9-S07-smoke-turn-allocate.md](P9-S07-smoke-turn-allocate.md)
