# P9-S02: Parser parity with Delta Chat core

## Action

Add `parse_turn_metadata(line) -> (host, port, expiry, password)` in `chatmail-turn` and tests that assert the same fields core derives in [`create_ice_servers_from_metadata`](../../../context/core/src/calls.rs).

Include golden lines:

- `127.0.0.1:3478:1758650868:8Dqkyyu11MVESBqjbIylmB06rv8=`
- Reject: missing field, bad port, non-numeric timestamp.

## Files touched

- `crates/chatmail-turn/src/parse.rs`
- `crates/chatmail-turn/src/lib.rs`

## TDD references

- [11-proxy-services.md](../../TDD/11-proxy-services.md)

## Madmail / context references

- `context/core/src/calls.rs` — `UnresolvedIceServer::Turn`

## RFC references

- [draft-uberti-behave-turn-rest-00](../../TDD/RFC/draft-uberti-behave-turn-rest-00.txt) — username as expiry

## Verification

**P9-UT02**

```bash
cargo test -p chatmail-turn p9_ut02
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P9-UT02** | `P9-S02` |

## Next

[P9-S03-config-turn-blocks.md](P9-S03-config-turn-blocks.md)
