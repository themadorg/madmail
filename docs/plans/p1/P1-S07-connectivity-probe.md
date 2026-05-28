# P1-S07: Connectivity probe & status

## Action

1. **Probe** on toggle enable + periodic scheduler:
   - `GET /webimap/mailboxes` → WebIMAP on/off
   - Track WebSMTP separately (probe via `POST /webimap/send` preflight or dedicated health if added)
2. Extend `src/scheduler/connectivity.rs` HTML:
   - `webimap_transport_enabled`
   - WS state: disconnected / connecting / connected
   - Server: webimap + websmtp probe result
   - Last push UID / last error
3. Optional JSON-RPC `get_webtransport_info` (nice-to-have).

## Files touched

- `src/webtransport/probe.rs`
- `src/scheduler/connectivity.rs`

## Tests (implement with this step)

| Test ID | Tier | Module | Asserts |
|---------|------|--------|---------|
| **P1-UT07** | Unit | `probe_tests.rs` | `ProbeResult::Disabled` on 404 body `not found`; `Enabled` on 200 + INBOX in JSON |
| **P1-UT08** | Unit | `connectivity_tests.rs` | `render_connectivity_html(...)` contains `WebIMAP` and `connected` when session mock says connected |
| **P1-E2E03** | E2E | `tests/chatmail_webtransport.rs` | Spawn chatmail with flags off → probe fails; CLI enable → probe succeeds |

### P1-UT07

```rust
#[tokio::test]
async fn p1_ut07_probe_disabled_on_404() { /* wiremock */ }

#[tokio::test]
async fn p1_ut07_probe_enabled_on_200() { /* wiremock */ }
```

### P1-UT08

```rust
#[tokio::test]
async fn p1_ut08_connectivity_html_shows_ws_state() { /* ... */ }
```

### P1-E2E03

```rust
#[tokio::test]
async fn p1_e2e03_probe_tracks_server_toggle() {
    // spawn chatmail, probe fail, chatmail ctl webimap enable via HTTP admin or env setup, probe ok
}
```

## Verification

```bash
cd context/core
cargo test p1_ut07
cargo test p1_ut08
CHATMAIL_WEBIMAP_TEST=1 cargo test p1_e2e03
```

**Step done when:** Connectivity dialog (manual P1-UI01) shows same strings as P1-UT08 HTML.

## Linked tests

| Test ID | Step |
|---------|------|
| P1-UT07 | P1-S07 |
| P1-UT08 | P1-S07 |
| P1-E2E03 | P1-S07, P1-S09 |

## Next

[P1-S08-desktop-toggle.md](P1-S08-desktop-toggle.md)
