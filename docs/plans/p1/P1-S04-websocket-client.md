# P1-S04: WebSocket client

## Action

Implement `WebtransportWs` in `context/core/src/webtransport/ws.rs`:

1. Build URL (from `WebimapBaseUrl` or auto):
   ```
   wss://{host}/webimap/ws?email={}&password={}&mailbox=INBOX&since_uid={}
   ```
2. Use `tokio-tungstenite` with TLS from Core net layer.
3. **Request/response:** `req_id` counter + `HashMap` pending (port `transport.ts` `wsRequest`).
4. **Push:** messages without `req_id` → `mpsc` channel to scheduler.
5. **Reconnect:** exponential backoff; on reconnect pass latest `since_uid`.
6. **Actions:** `list_mailboxes`, `list_messages`, `fetch`, `delete`, `flags`, `send`.

Run WS loop on dedicated tokio task; `Drop` closes cleanly.

## Security note

Password in query string is **spec-required** today. Log URLs with redacted password.

## Reference

- `crates/chatmail-www/src/webimap_ws.rs`
- `desktop/protocol/websocket_spec.md`

## Tests (implement with this step)

| Test ID | Tier | Module | Asserts |
|---------|------|--------|---------|
| **P1-UT03** | Unit | `ws_tests.rs` | In-process mock: send client JSON with `req_id`, mock server replies `result` → pending future resolves |
| **P1-UT03b** | Unit | `ws_tests.rs` | Server sends push without `req_id` → `mpsc` receives `MessageSummary` |
| **P1-UT04** | Unit | `ws_tests.rs` | On disconnect, pending requests fail with `WsDisconnected`; backoff sequence `1s,2s,4s` capped at `30s` (inject clock or test helper) |

Use `tokio::test` + local `tokio-tungstenite` server (see iroh-relay test patterns) or `tokio::io::duplex` framing shim.

### P1-UT03

```rust
#[tokio::test]
async fn p1_ut03_ws_request_response_correlation() { /* ... */ }
```

### P1-UT03b

```rust
#[tokio::test]
async fn p1_ut03b_ws_push_dispatched_to_channel() { /* ... */ }
```

### P1-UT04

```rust
#[tokio::test]
async fn p1_ut04_ws_disconnect_fails_pending() { /* ... */ }

#[test]
fn p1_ut04_reconnect_backoff_sequence() { /* pure fn */ }
```

## Verification

```bash
cd context/core
cargo test p1_ut03
cargo test p1_ut03b
cargo test p1_ut04
```

**Step done when:** all three pass; logs never print raw password.

## Linked tests

| Test ID | Step |
|---------|------|
| P1-UT03 | P1-S04 |
| P1-UT03b | P1-S04 |
| P1-UT04 | P1-S04 |

## Next

[P1-S05-scheduler-receive.md](P1-S05-scheduler-receive.md)
