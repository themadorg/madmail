# 09 — Federation: Inbound (`/mxdeliv`) and Outbound Delivery

Federation is how one chatmail server delivers mail to another without the sender having an account on the destination server.

This is the feature that turns "personal mail server" into a **federated** secure messenger network.

## The Two Directions

### Inbound (remote server → this server)

Primary path: `POST /mxdeliv` (HTTP)

Fallback: SMTP to port 25

### Outbound (this server → remote server)

The `chatmail-delivery` queue tries:
1. HTTPS POST /mxdeliv (using discovered or configured endpoint)
2. Plain HTTP
3. Traditional SMTP (MX lookup + STARTTLS)

All paths are tracked for latency and failure rate.

## Inbound: `POST /mxdeliv` Handler

Location: `crates/chatmail-fed/src/mxdeliv.rs`

Flow (simplified):

1. Extract `X-Mail-From` header (the claimed sender).
2. Read the entire body as raw message bytes.
3. **Recipient validation**:
   - Header `X-Chatmail-Recipient` or `X-Recipient` or the path in the request?
   - `recipient_matches_server` + `inbound_local_recipient_allowed`
4. **Blocklist check** on the sender (some servers maintain per-sender blocks).
5. **PGP gate** (`chatmail_pgp::enforce_encryption`) — same rules as local submission.
6. **Policy check** — `FederationPolicyCache` (ACCEPT/REJECT/SILENT_DISMISS per domain or pattern).
7. **Quota check** — `AppState::quota.check_quota`.
8. **Local delivery** — `chatmail_storage::deliver_local_messages` (writes to Maildir, updates quota in RAM).
9. **Notify IDLE** — broadcast on the EventBus so connected IMAP clients wake up.
10. Record stats (`record_inbound_delivery`).

HTTP status codes are mapped deliberately:
- 200 OK → accepted
- 403 Forbidden → policy or encryption rejection
- 507 Insufficient Storage → quota exceeded

The body on error is minimal ("Forbidden", "quota", "bad request") to avoid leaking details.

## Federation Policy & Silent Dismiss

There are two related but distinct mechanisms:

- `federation_policy` table / cache → explicit ACCEPT or REJECT rules (domain, IP, etc.).
- `federation_silent_dismiss` → a special mode where the server pretends to accept the mail (returns 200) but then immediately discards it. Used for spam mitigation without giving the remote server useful feedback.

Both are hydrated into `AppState` and checked on the hot path.

## Outbound: The Delivery Queue

Location: `crates/chatmail-delivery/`

Started once in `supervisor.rs`:

```rust
let queue = start_outbound_queue(delivery_ctx, state_dir, &config.queue).await?;
```

Components:
- `queue/store.rs` — persistent job storage (SQLite-backed or file-based queue)
- `queue/worker.rs` — N parallel workers that pick jobs and attempt delivery
- `queue/config.rs` — `max_tries`, `max_parallelism`, backoff, etc.
- `router.rs` — decides which transport to try for a given recipient domain
- `transport.rs` + `federation_http.rs` — the actual HTTP and SMTP senders

### Retry & Backoff

A job is retried up to `max_tries`. After permanent failure (or max tries) it is moved to a dead-letter area or logged.

Stats are recorded into the `FederationTracker` (success, failure, latency) which is periodically flushed to the DB and visible in the admin UI.

### Endpoint Discovery & Overrides

For a destination domain the router may:
- Use a cached endpoint from previous successful deliveries (`endpoint_cache`).
- Respect `dns_overrides` table (admin can force a particular host for a domain).
- Fall back to `https://domain/.well-known/chatmail` or similar discovery (if implemented).
- Finally fall back to classic MX + SMTP.

This is one of the places where chatmail federation is more robust and faster than pure SMTP.

## The `X-Mail-From` Header (Important Detail)

Because the HTTP POST path does not go through a full SMTP envelope, the original sender address is passed in the `X-Mail-From` header.

The receiving server trusts this header (it came from another chatmail server that already performed its own authentication and PGP checks).

This is analogous to how internal mail relays trust each other.

## Anti-Enumeration & Privacy

- The inbound handler is careful not to reveal whether a recipient exists on policy failures (some paths return the same error for "user unknown" and "policy reject").
- Silent dismiss exists precisely so that an attacker cannot use HTTP status codes or timing to map the user namespace.

## FederationTracker (Observability)

`chatmail_state::tracker::FederationTracker`

Per-destination-domain counters:
- attempts, successes, failures
- last latency, last error, etc.

Flushed every ~30s by the background flusher.

Visible in admin UI under federation stats. Used for operational decisions ("this peer is flaky, maybe switch to SMTP fallback only").

## Testing Federation

- Unit tests in the fed and delivery crates.
- E2E tests that stand up two chatmail instances and send between them.
- `tests/securejoin_e2e.rs` and Delta Chat p2p tests exercise the full path.

## Relationship to the Original Madmail

The Go implementation had an `internal/target/remote` and `filtermail` component that did similar routing + HTTP POST + SMTP fallback.

The Rust version deliberately mirrors the observable behavior (same error strings where possible, same policy semantics) so that mixed Go/Rust federations work seamlessly.

## Next

Federation and delivery are how mail moves between servers. The next layer is the **public web surface and the operator admin tools**.

→ [10-web-services-and-admin.md](./10-web-services-and-admin.md)
