# websocket-bridge

Expose native IMAP and authenticated SMTP submission over secure WebSocket.
Disabled by default; this is separate from the JSON WebIMAP API.

```bash
madmail websocket-bridge status
madmail websocket-bridge enable
madmail websocket-bridge disable
madmail --json websocket-bridge status
```

A saved CLI/Admin API value takes precedence over the static config fallback.
Set `websocket_bridge_enabled = true` in TOML, or
`websocket_bridge_enabled true` inside the maddy `chatmail { ... }` block, to
provide that fallback. Config changes require a restart; CLI/API toggles apply to
new connections immediately. Disabling does not disconnect existing sessions.

The **Services → WebSocket mail bridge** dashboard toggle uses Admin RPC resource
`/admin/services/websocket_bridge` (GET status, POST `{"action":"enable"}` or
`{"action":"disable"}`). Invoke it through the configured admin RPC endpoint
with the normal admin Bearer token in the RPC envelope’s `headers.Authorization`; the resource is not a public HTTP route.
`GET /admin/settings` includes `websocket_bridge_enabled` (`enabled`/`disabled`).

## Client contract

Connect to `wss://mail.example/transport/imap` or
`wss://mail.example/transport/smtp`. Each socket is one ordered, bidirectional byte
stream connected to Madmail's own protocol session. Server greetings arrive first.
Use **binary** WebSocket messages, at most 64 KiB each, and chunk larger writes.
Message boundaries need not match command or literal boundaries. Preserve raw bytes,
including CRLF, literal lengths and SMTP dot-stuffing. Text messages are rejected.
Authenticate using ordinary IMAP LOGIN/AUTHENTICATE or SMTP AUTH inside the stream.
SMTP always requires authentication and uses existing submission policy. Do not
issue STARTTLS inside WSS: confidentiality is already established by WSS.

This needs a client transport adapter; ordinary mail clients cannot use a WSS URL
directly. See the Rust client in `tests/websocket_bridge_e2e.rs` for a working
example, including TLS certificate verification, SMTP delivery and IMAP IDLE.
Query parameters and arbitrary target addresses are rejected. Do not place
passwords in URLs. PGP/SecureJoin, blocked-account, sender, size and quota rules
remain those of the native services.

## TLS, browser origins and reverse proxies

Madmail itself must accept the backend connection over **HTTPS**. Plain HTTP is
rejected even with `X-Forwarded-Proto: https`. A reverse proxy terminating public
TLS must also use HTTPS to Madmail, preserve WebSocket upgrade headers and allow
long-lived connections (over 35 minutes for IMAP IDLE). Configure certificate
verification for that upstream; do not disable it for production.

Non-browser clients can omit Origin. For browsers, explicitly allow the client:

```bash
madmail webmail-cors add https://client.example
```

The shared origin list is used, but the bridge does **not** accept `*` or implicit
WebIMAP origin reflection. A browser can forward native protocol bytes; no mail
credentials need to be sent in the upgrade URL or headers.

## Limits and troubleshooting

- Disabled → HTTP 404; HTTP or disallowed Origin → 403; any query string → 400.
- 503 → connection capacity reached or native session configuration unavailable.
- 256 active/pending bridge connections per process; 60 seconds to authenticate.
- 35 minutes of read inactivity; writes stall out after 30 seconds.
- Socket closure, protocol failure or listener shutdown releases the session.

For failed upgrades, check the service status, backend HTTPS, exact Origin and
proxy upgrade support. Large payloads must be split into messages no larger than
64 KiB. Ping/pong keeps the transport alive without entering the mail stream.

## Validation

```bash
cargo test -p chatmail-integration --test websocket_bridge_e2e
make test-websocket-bridge-docker
```

The Docker runner builds the shipping image, uses fresh generated TLS and isolated
state, runs the Rust client, checks live CLI/API controls and removes its container
and temporary files. Set `MADMAIL_BRIDGE_TEST_SKIP_BUILD=1` only to reuse an already
built matching `MADMAIL_BRIDGE_TEST_IMAGE`.

Design: [TDD](../../TDD/26-websocket-mail-bridge.md).
