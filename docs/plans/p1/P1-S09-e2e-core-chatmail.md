# P1-S09: Core E2E tests vs madmail-v2

## Action

Centralize subprocess E2E in `context/core/src/tests/chatmail_webtransport.rs` (reuse `chatmail_transport.rs` helpers).

| Test ID | Function name | Flow |
|---------|---------------|------|
| **P1-E2E01** | `p1_e2e01_receive_over_websocket` | Register, SMTP inject, WS push + fetch, `get_msg_cnt` > 0 |
| **P1-E2E02** | `p1_e2e02_p2p_send_via_webtransport` | Two users, Alice sends PGP via WS, Bob receives |
| **P1-E2E03** | `p1_e2e03_probe_tracks_server_toggle` | Probe 404 → enable `WEBIMAP_ENABLED` → probe 200 |

All gated:

```rust
if std::env::var("CHATMAIL_WEBIMAP_TEST").ok().as_deref() != Some("1") {
    return Ok(());
}
```

Spawn helper (extend `chatmail_transport.rs`):

```rust
pub async fn spawn_chatmail_webimap_enabled() -> Result<ChatmailChild> {
    let child = spawn_chatmail().await?;
    // POST admin or direct DB: WEBIMAP_ENABLED + WEBSMTP_ENABLED = true
    // (mirror madmailv2 tests/support/mod.rs lines 90–93)
    Ok(child)
}
```

Add `madmailv2/scripts/core-e2e-webimap.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cargo build -p chatmail --manifest-path "$ROOT/Cargo.toml"
export CHATMAIL_BIN="$ROOT/target/debug/chatmail"
export CHATMAIL_WEBIMAP_TEST=1
CORE="${CORE:-$ROOT/../desktop/core}"
cd "$CORE"
cargo test p1_e2e -- --nocapture
```

## Files touched

- `context/core/src/tests/chatmail_webtransport.rs`
- `context/core/src/tests/chatmail_transport.rs` — shared spawn + HTTP helpers
- `madmailv2/scripts/core-e2e-webimap.sh`

## Tests (this step owns E2E suite)

| Test ID | Tier | Depends on steps |
|---------|------|------------------|
| P1-E2E01 | E2E | S04, S05 |
| P1-E2E02 | E2E | S04, S06 |
| P1-E2E03 | E2E | S03, S07 |

### P1-E2E01 detail

- Enable `webimap_transport_enabled` on test context.
- `smtp_deliver` PGP test message to INBOX.
- Wait for WS `new_message` or poll REST (max 30s).
- Assert message text / chat exists.

### P1-E2E02 detail

- Two `http_register` users on same server.
- Configure both contexts with transport on.
- Send encrypted test mail A→B via `WebtransportWs::send`.
- B: `wait_for_msgs` or poll DB.

### P1-E2E03 detail

- `probe()` before enable → `Disabled`.
- Run `chatmail webimap enable` via `Command` or test-only admin API.
- `probe()` → `Enabled`.

## Verification

```bash
cd madmailv2
chmod +x scripts/core-e2e-webimap.sh
./scripts/core-e2e-webimap.sh
```

Or individually:

```bash
export CHATMAIL_WEBIMAP_TEST=1 CHATMAIL_BIN=madmailv2/target/debug/chatmail
cd context/core
cargo test p1_e2e01 -- --nocapture
cargo test p1_e2e02 -- --nocapture
cargo test p1_e2e03 -- --nocapture
```

**Step done when:** all three E2E tests pass locally with built `chatmail` binary.

## Linked tests

| Test ID | Step |
|---------|------|
| P1-E2E01 | P1-S05, P1-S09 |
| P1-E2E02 | P1-S06, P1-S09 |
| P1-E2E03 | P1-S07, P1-S09 |

## Next

[P1-S10-server-operator-runbook.md](P1-S10-server-operator-runbook.md)
