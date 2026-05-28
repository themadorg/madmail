# P1-S02: WebIMAP protocol types in Core

## Action

Port JSON shapes from:

- `desktop/deltachat-web-mono/packages/sdk/lib/transport.ts`
- `context/madmail/docs/chatmail/webimap.md`
- `crates/chatmail-www/src/webimap.rs` (server `Serialize` types)

Create `context/core/src/webtransport/types.rs`:

| Type | Fields (match server) |
|------|------------------------|
| `WsRequest` | `req_id`, `action`, `data` |
| `WsResponse` | `req_id`, `action`, `data` |
| `WsPush` | `action: "new_message"`, `data: MessageSummary` |
| `MessageSummary` | `uid`, `envelope`, … |
| `MessageDetail` | summary + `body` (raw MIME) |
| `WebimapSendRequest` | `from`, `to`, `body` |

Add `fixtures/` JSON files copied from `chatmail-www` tests or madmail doc examples.

## Files touched

- `context/core/src/webtransport/types.rs`
- `context/core/src/webtransport/types_tests.rs` (or `#[cfg(test)]` mod)
- `context/core/src/webtransport/fixtures/*.json`
- `context/core/src/webtransport/mod.rs`
- `context/core/src/lib.rs` — `mod webtransport;`

## Tests (implement with this step)

| Test ID | Tier | Module | Asserts |
|---------|------|--------|---------|
| **P1-UT01** | Unit | `types_tests.rs` | Deserialize fixture `ws_list_messages_result.json` → `Vec<MessageSummary>`; serialize `WsRequest` matches golden string |
| **P1-UT01b** | Unit | `types_tests.rs` | Deserialize `ws_new_message_push.json` → `WsPush` with `action == "new_message"` and `uid > 0` |

### Fixtures (minimum)

| File | Source |
|------|--------|
| `fixtures/ws_request_send.json` | `websocket_spec.md` send example |
| `fixtures/ws_response_result.json` | `{ "req_id":"1", "action":"result", "data":[] }` |
| `fixtures/ws_new_message_push.json` | TDD 10-webimap push example |
| `fixtures/rest_mailboxes.json` | `[{"name":"INBOX","messages":0,"unseen":0}]` |

### P1-UT01

```rust
#[test]
fn p1_ut01_ws_request_roundtrip() { /* ... */ }

#[test]
fn p1_ut01_message_summary_from_server_json() { /* ... */ }
```

### P1-UT01b

```rust
#[test]
fn p1_ut01b_new_message_push_without_req_id() { /* ... */ }
```

## Verification

```bash
cd context/core
cargo test p1_ut01
cargo test p1_ut01b
```

**Step done when:** fixtures committed; no `serde` unknown-field failures against real server samples.

## Linked tests

| Test ID | Step |
|---------|------|
| P1-UT01 | P1-S02 |
| P1-UT01b | P1-S02 |

## Next

[P1-S03-rest-client.md](P1-S03-rest-client.md)
