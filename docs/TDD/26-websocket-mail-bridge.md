# Raw mail WebSocket bridge

Issue: https://github.com/themadorg/madmail/issues/224

## Contract

The single binary exposes `GET /transport/imap` and `GET /transport/smtp` as binary
WebSocket byte streams. Each socket owns one native IMAP or authenticated SMTP
submission session. Message boundaries have no protocol meaning; bytes, CRLF,
IMAP literals and SMTP dot-stuffing pass through unchanged. Text frames are rejected.
No destination address is accepted. Existing `/webimap/ws` JSON behavior is unchanged.

## TLS and policy

Only connections accepted by Madmail's TLS HTTP driver receive `SecureHttp` and may
upgrade. HTTP and forged forwarding headers cannot establish this boundary. Reverse
proxies must connect to Madmail over HTTPS, including when terminating public TLS.
The bridge calls explicit secure-transport session entry points with STARTTLS
turned off and a fresh greeting. It uses the supervisor's own session configurations,
including `require_auth = true` for SMTP; normal auth, JIT, quota, sender checks,
PGP/SecureJoin and delivery remain in the native protocol implementation.

Browser Origins require an exact entry in `__WEBMAIL_CORS_ORIGINS__`; wildcard and
WebIMAP auto-reflection do not apply. Clients without Origin are permitted and still
must authenticate through the mail protocol. Credentials belong in protocol bytes,
never the request URL. Request queries are rejected. Payloads and credentials are
never logged by the bridge.

## Controls

Disabled by default. `websocket_bridge_enabled = true` in TOML or
`websocket_bridge_enabled true` inside a maddy `chatmail` block supplies the static
fallback. `__WEBSOCKET_BRIDGE_ENABLED__` in the settings DB overrides that fallback.
`madmail websocket-bridge enable|disable|status`, Admin RPC resource
`/admin/services/websocket_bridge`, and the admin web service toggle share the DB
setting. POST takes `{"action":"enable"}` or `{"action":"disable"}`. Disabled
routes return 404. Changes affect new connections; existing sessions finish normally.

## Resource limits and lifecycle

A process-wide maximum of 256 bridge connections includes pending upgrades.
Binary messages and frames are limited to 64 KiB; the client must chunk larger protocol data. A 64 KiB duplex buffer bounds
in-process flow, with 16 KiB outbound chunks. Writes time out after 30 seconds;
read inactivity after 35 minutes accommodates IMAP IDLE. Authentication must finish
within 60 seconds regardless of ping traffic. Ping/pong is handled separately from
mail bytes. Closing either side cancels the other, drops the session and releases
the permit. Listener shutdown cancels active bridge sessions. No detached mail task
survives a socket. Existing native parser and message-size limits still apply.

## Verification

Unit/integration tests exercise default-off and overrides, plaintext/forwarded
header rejection, origin and query restrictions, binary framing and byte integrity,
normal auth and submission policy, timeout/close cleanup and listener shutdown.
`tests/websocket-bridge-docker.sh` runs a Rust WebSocket client against an isolated
Docker server with generated TLS, then cleans up all containers and temporary state.
The client exercises SMTP-to-IMAP delivery, fragmented commands, literals, IDLE,
auth failures, plaintext rejection, service controls and independent native IMAP
visibility. Follow CONTRIBUTING.md's fmt, clippy and workspace test requirements.

The `WebSocket mail bridge E2E` GitHub Actions workflow runs the shipping-image
scenario for relevant pull requests and supports manual dispatch.


## Destination isolation regression

The Rust client attempts arbitrary destination queries, protocol selection, unknown
transport routes, Host/forwarding/subprotocol overrides and HTTP CONNECT/GET,
Redis and SQL commands inside both mail sockets. Each remains rejected by the
HTTP gate or the selected native mail parser. A TCP canary must receive no
connections from these attempts; the Docker test first reaches that canary from
inside the server container as a positive control. Legitimate authenticated SMTP
mail delivery retains its normal routing semantics; the bridge is not a general
TCP, HTTP CONNECT or SOCKS proxy.
