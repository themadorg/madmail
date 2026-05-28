# P1-S03: WebIMAP REST client

## Action

Implement `WebtransportRest` in `context/core/src/webtransport/rest.rs`:

| Method | HTTP | Notes |
|--------|------|-------|
| `probe()` | `GET /webimap/mailboxes` | 200 = enabled; 404 = server disabled |
| `list_messages(since_uid, wait)` | `GET /webimap/messages?...` | Long-poll optional |
| `fetch_message(uid)` | `GET /webimap/message/{uid}?mailbox=INBOX` | Returns MIME in `body` |
| `send_mime(to, body)` | `POST /webimap/send` | JSON; needs WebSMTP enabled |
| `delete_message(uid)` | `DELETE /webimap/message/{uid}` | Optional P1 |

Auth headers on every call:

```
X-Email: {configured_addr}
X-Password: {configured_password}
```

Reuse Core HTTP stack (`reqwest` with proxy + cert checks from transport settings).

Extend `context/core/src/tests/chatmail_transport.rs`:

- `http_fetch_message`, `http_send`, `http_probe_disabled` helpers

## madmailv2 reference

- `crates/chatmail-www/src/webimap.rs`
- `tests/support/mod.rs` `webimap_send`

## Tests (implement with this step)

| Test ID | Tier | Module | Asserts |
|---------|------|--------|---------|
| **P1-UT02** | Unit | `rest_tests.rs` | `auth_headers()` sets `X-Email` / `X-Password`; `base_url` joins path `/webimap/mailboxes` |
| **P1-UT02b** | Unit | `rest_tests.rs` + `wiremock` | Mock 200 + `rest_mailboxes.json` body → `probe()` returns `Ok(())`; mock 404 → `ProbeDisabled` |
| **P1-IT01** | Integration | `crates/chatmail-www` | Existing server tests still pass (no regression) |

### P1-UT02

```rust
#[test]
fn p1_ut02_rest_builds_auth_headers() { /* ... */ }

#[test]
fn p1_ut02_rest_url_joins_webimap_path() { /* ... */ }
```

### P1-UT02b

```rust
#[tokio::test]
async fn p1_ut02b_probe_ok_on_200() { /* wiremock */ }

#[tokio::test]
async fn p1_ut02b_probe_err_on_404() { /* wiremock */ }
```

### P1-IT01 (madmailv2 — run in same PR if touching server)

```bash
cargo test -p chatmail-www
cargo test -p chatmail-integration webimap -- --test-threads=1
```

## Verification

```bash
cd context/core && cargo test p1_ut02 && cargo test p1_ut02b
cd madmailv2 && cargo test -p chatmail-www
```

**Step done when:** P1-UT02, P1-UT02b, P1-IT01 green; `chatmail_transport.rs` gains `http_fetch_message` used by P1-E2E01 later.

## Linked tests

| Test ID | Step |
|---------|------|
| P1-UT02 | P1-S03 |
| P1-UT02b | P1-S03 |
| P1-IT01 | P1-S03 |

## Next

[P1-S04-websocket-client.md](P1-S04-websocket-client.md)
