# P9-S10: E2E — Delta Chat core `ice_servers()`

## Action

Validate the **full client path** from IMAP metadata to Core JSON, using the same subprocess pattern as [`context/core/src/tests/chatmail_transport.rs`](../../../context/core/src/tests/chatmail_transport.rs) and [`chatmail_rs_p2p.rs`](../../../context/core/src/tests/chatmail_rs_p2p.rs).

### 1. chatmail subprocess with TURN

Extend test config written by `spawn_chatmail()` (or new `spawn_chatmail_with_turn()`):

- `turn_enable = true`
- `turn_secret` fixed test value
- TURN listens on ephemeral port; IMAP `turn_port` matches
- Env: `CHATMAIL_TURN_ADDR` (if needed)

### 2. Core test (in `context/core` or madmailv2 wrapper)

Add `chatmail_rs_ice_servers_from_metadata`:

1. Register account via HTTP `/new` (existing transport).
2. Configure IMAP account on spawned chatmail.
3. Trigger `update_metadata` / scheduler sync.
4. Call `ice_servers(&ctx)`.
5. Assert JSON contains `turn:127.0.0.1:<port>` (or resolved IP) with `username` + `credential`.
6. Assert **no** `turn.delta.chat` in output (fallback not used).

### 3. Script

Add [`scripts/core-e2e-turn.sh`](../../../scripts/core-e2e-turn.sh):

```bash
#!/usr/bin/env bash
# Core E2E: chatmail with TURN → ice_servers() uses local relay
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export CHATMAIL_BIN="${CHATMAIL_BIN:-$ROOT/target/debug/chatmail}"
export CHATMAIL_TURN_TEST=1
cargo build -p chatmail -q
cd "${CORE_DIR:-$ROOT/../../desktop/core}"
cargo test chatmail_rs_ice_servers_from_metadata -- --nocapture "$@"
```

### 4. Video call (manual / nightly)

Document checklist for two DC clients on same server — not blocking Phase 9 CI until WebRTC harness exists.

## Files touched

- `context/core/src/tests/chatmail_turn.rs` (new, `#[ignore]` without `CHATMAIL_TURN_TEST`)
- `scripts/core-e2e-turn.sh`
- `Makefile` — `test-core-turn`
- `docs/TDD/16-testing.md` — add scenario #18 TURN/ICE

## TDD references

- [11-proxy-services.md](../../TDD/11-proxy-services.md)
- [16-testing.md](../../TDD/16-testing.md)

## Madmail / context references

- `context/core/src/imap.rs` — `update_metadata`
- `context/core/src/calls.rs` — `ice_servers`, `create_fallback_ice_servers`
- `scripts/core-e2e.sh` — template

## RFC references

- [RFC 5464](../../TDD/RFC/rfc5464.txt) — discovery
- [RFC 8445](../../TDD/RFC/rfc8445.txt) — ICE server list consumption

## Verification

**P9-E2E03**

```bash
scripts/core-e2e-turn.sh
# or inside core tree:
CHATMAIL_TURN_TEST=1 cargo test chatmail_rs_ice_servers_from_metadata
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P9-E2E03** | `P9-S10` |

## Next

Phase 9 complete when [README](README.md) test matrix is green.
