# 08 — SMTP and IMAP Servers (The Protocol Engines)

These two crates (`chatmail-smtp` and `chatmail-imap`) are where the "mail server" part of chatmail actually speaks the protocols that Delta Chat (and normal MUAs) expect.

## Design Choice: Custom Async Implementations

Unlike many servers that use a third-party SMTP/IMAP library for the protocol framing, madmail-v2 wrote its own async state machines.

Reasons (from the TDD and plans):
- Full control over the exact error messages and timing (important for "No-Log" and PGP rejection UX).
- Ability to integrate the PGP gate, JIT auth, quota, and federation policy at exactly the right points.
- Simpler integration with the in-memory `AppState` and the delivery pipeline.
- Easier to add Chatmail-specific extensions (METADATA for TURN, etc.).

The implementations are **not** full RFC-complete mail servers. They implement the subset needed for Delta Chat + federation.

## SMTP (`chatmail-smtp`)

### Structure

- `server.rs` — binds the listener(s), spawns per-connection tasks.
- `session.rs` — `SmtpSession` state machine. The heart.
- `protocol.rs` — low-level command and response parsing.
- `data_limit.rs` — message size enforcement (before and after DATA).

Two different `SmtpSessionConfig` instances are created at supervisor start:
- Inbound (port 25): `require_auth = false`
- Submission (465/587): `require_auth = true`

### Session Lifecycle (high level)

1. TCP accept → new `SmtpSession`.
2. Greeting (220).
3. EHLO (advertises STARTTLS, SIZE, AUTH, etc.).
4. AUTH (on submission) → calls into `chatmail_auth::authenticate`.
5. MAIL FROM.
6. RCPT TO (multiple) — each checked against local domains + blocklist + quota (rough).
7. DATA → streaming the message body.
   - Size limit checked.
   - **PGP gate** (`chatmail_pgp::enforce_encryption`) is applied here.
   - If passes → handed to local delivery or outbound queue.
8. QUIT or connection close.

### Local vs Remote Recipients

- Local recipients (matching `local_domains` or JIT domain) → delivered via `chatmail_storage::deliver_local_messages`.
- Remote recipients → enqueued in the outbound delivery queue (`chatmail-delivery`).

One message can have a mix (rare in practice for chatmail usage).

### Key Integration Points

- `AppState::check_message_size`
- `AppState::quota.check_quota`
- `chatmail_pgp::enforce_encryption`
- `chatmail_db::inbound_local_recipient_allowed`
- Event bus notification for IMAP IDLE after local delivery

## IMAP (`chatmail-imap`)

### Structure

Similar pattern:
- `server.rs`
- `session.rs` — large command dispatch table
- `connection_stats.rs`

### Supported Commands (the ones Delta Chat actually uses)

From the integration tests and TDD:
- CAPABILITY, ID, LOGIN / AUTHENTICATE (including XOAUTH2 path in some setups)
- LIST, SELECT (with CONDSTORE support bits)
- FETCH, STORE, EXPUNGE, CLOSE
- IDLE (the push mechanism — critical)
- MOVE, APPEND
- GETQUOTA / GETQUOTAROOT
- GETMETADATA / SETMETADATA (the Chatmail magic for TURN/Iroh discovery)
- STATUS, etc.

### IDLE & Push

When a client does `IDLE`, the session registers with the `EventBus` (in `AppState`).

When a new message is delivered locally (via SMTP or /mxdeliv), an event is broadcast. All IDLE sessions for that user wake up and send `EXISTS` + `RECENT` to the client.

This gives near-instant push without polling.

### METADATA Extension (the TURN/Iroh secret)

Delta Chat clients ask for specific server entries via `GETMETADATA (server)` or per-mailbox.

The IMAP server populates these from the `ImapSessionConfig` that was built at supervisor start time, which contains the `TurnDiscovery` and `IrohDiscovery` info.

This is how a Delta Chat client learns "the TURN server for calls on this account is at `turn@host:port` with this secret".

No extra protocol or out-of-band channel needed.

### Quota

IMAP `GETQUOTA` reads from the in-memory `QuotaCache` (which is kept in sync with actual Maildir usage + the `quotas` table).

## Shared Concerns

### TLS

Both servers use `chatmail_tls::load_server_config` (rustls) when the listener is a TLS port.

Plain ports can also do STARTTLS where supported by the protocol.

### Session Config vs Per-Connection State

The `SmtpSessionConfig` / `ImapSessionConfig` are relatively static (domain list, credential policy, discovery info).

Per-connection state (authenticated user, selected mailbox, IDLE state, etc.) lives in the session struct.

### Error Handling & Privacy

Rejection messages are crafted to leak as little information as possible (especially under No-Log mode).

PGP rejection is a specific, user-visible error that Delta Chat understands.

## Testing These Crates

- Unit tests inside the crates (protocol parsing, PGP gate, etc.).
- `cargo test -p chatmail-imap`
- Full E2E in `tests/imap_e2e.rs`, `tests/securejoin_e2e.rs`, `tests/deltachat_p2p_e2e.rs` (these actually speak the protocols against a booted server).

## Where the Real "Business Logic" Happens

The SMTP/IMAP crates are mostly **protocol glue**.

The interesting decisions are made in:
- `chatmail_auth`
- `chatmail_pgp`
- `chatmail_state` (quota, policy, events)
- `chatmail_storage`
- `chatmail_delivery` (for outbound RCPT)
- `chatmail_db` (recipient validation, blocklist)

If you are debugging "why was this message rejected?", start from the session, then follow the calls into the above crates.

## Next

Now that you understand the protocol servers, the next critical piece is **how messages get from one chatmail server to another**.

→ [09-federation-inbound-outbound.md](./09-federation-inbound-outbound.md)
