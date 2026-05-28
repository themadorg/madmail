# P1-S06: Scheduler — send path

## Action

When WebIMAP transport active and server WebSMTP enabled:

1. Intercept outbound MIME jobs that today go to SMTP submission.
2. Prefer WS `send` action `{ to, body }` (`from` forced server-side).
3. On WS down: `POST /webimap/send`.
4. Map errors to existing SMTP UX (`Encryption Needed`, quota 552) via `web_delivery_error` status codes.

Keep **SMTP** fallback when websmtp disabled, send fails once, or Bcc semantics incompatible.

## madmailv2 reference

- `websmtp_deliver` in `crates/chatmail-www/src/handlers.rs`

## Tests (implement with this step)

| Test ID | Tier | Module | Asserts |
|---------|------|--------|---------|
| **P1-UT06** | Unit | `send_tests.rs` | Map 400 + `"Encryption Needed"` body → same user string as SMTP path; 404 → fallback flag |
| **P1-IT03** | Integration | `webtransport_integration_tests.rs` | wiremock WS: `send` action returns `{status:"sent"}`; scheduler marks job done without SMTP mock called |
| **P1-E2E02** | E2E | `tests/chatmail_webtransport.rs` | Alice sends PGP mail to Bob via WS/REST; Bob receives (IMAP or WS) |

### P1-UT06

```rust
#[test]
fn p1_ut06_maps_encryption_needed_from_400() { /* ... */ }

#[test]
fn p1_ut06_maps_quota_from_413() { /* ... */ }
```

### P1-IT03

```rust
#[tokio::test]
async fn p1_it03_outbound_uses_ws_when_connected() { /* ... */ }
```

### P1-E2E02

```rust
#[tokio::test]
async fn p1_e2e02_p2p_send_via_webtransport() { /* CHATMAIL_WEBIMAP_TEST=1 */ }
```

## Verification

```bash
cd context/core
cargo test p1_ut06
cargo test p1_it03
CHATMAIL_WEBIMAP_TEST=1 cargo test p1_e2e02
```

**Step done when:** P1-UT06 + P1-IT03 in default `cargo test`; P1-E2E02 in webimap e2e script.

## Linked tests

| Test ID | Step |
|---------|------|
| P1-UT06 | P1-S06 |
| P1-IT03 | P1-S06 |
| P1-E2E02 | P1-S06, P1-S09 |

## Next

[P1-S07-connectivity-probe.md](P1-S07-connectivity-probe.md)
