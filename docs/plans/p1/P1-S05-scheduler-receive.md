# P1-S05: Scheduler — receive path

## Action

Wire WebIMAP into message **ingest** without rewriting `imap.rs`:

1. `context/core/src/webtransport/session.rs` — owns `WebtransportWs` + `WebtransportRest`.
2. On scheduler tick / account start: if `webimap_transport_eligible()`, spawn session.
3. On `new_message` push (or REST poll gap-fill):
   - `fetch` UID → `MessageDetail.body` (raw RFC822)
   - Call existing receive path (`receive_imf` / same as IMAP FETCH body)
4. Update `imap_sync` UID cursor for INBOX (`set_uid_next` / `get_uid_next`).
5. Call `idle_interrupted` / connectivity `set_connected`.

**Hybrid:** reduce IMAP poll frequency when WS healthy (e.g. 15 min backup poll).

## Files touched

- `src/webtransport/session.rs`
- `src/webtransport/scheduler_hook.rs`
- `src/scheduler/` (integration point)
- `src/imap.rs` — shared UID helpers only

## Tests (implement with this step)

| Test ID | Tier | Module | Asserts |
|---------|------|--------|---------|
| **P1-UT05** | Unit | `session_tests.rs` | After handling push UID 5, `get_uid_next(INBOX)` == 6 (mock ingest, no network) |
| **P1-IT02** | Integration | `webtransport_integration_tests.rs` | Fake push + fixture MIME → one row in `msgs` table; `idle_interrupted` event emitted (test channel) |
| **P1-E2E01** | E2E | `tests/chatmail_webtransport.rs` | Real `chatmail` subprocess: SMTP inject → WS `new_message` → message in Core DB |

### P1-UT05

```rust
#[tokio::test]
async fn p1_ut05_uid_cursor_advances_after_ingest() { /* ... */ }
```

### P1-IT02

```rust
#[tokio::test]
async fn p1_it02_push_triggers_ingest_and_event() {
    // Use Context::new_closed with prefilled config, mock WebtransportWs push channel
}
```

### P1-E2E01 (requires S03–S04 + subprocess)

```rust
#[tokio::test]
async fn p1_e2e01_receive_over_websocket() {
    if std::env::var("CHATMAIL_WEBIMAP_TEST").ok().as_deref() != Some("1") {
        return Ok(());
    }
    // spawn_chatmail, enable webimap, register, smtp_deliver, enable transport, assert msg count
}
```

## Verification

```bash
cd context/core
cargo test p1_ut05
cargo test p1_it02
CHATMAIL_WEBIMAP_TEST=1 cargo test p1_e2e01
```

**Step done when:** P1-UT05 + P1-IT02 always run in CI; P1-E2E01 in `core-e2e-webimap.sh` nightly or optional job.

## Linked tests

| Test ID | Step |
|---------|------|
| P1-UT05 | P1-S05 |
| P1-IT02 | P1-S05 |
| P1-E2E01 | P1-S05, P1-S09 |

## Next

[P1-S06-scheduler-send.md](P1-S06-scheduler-send.md)
