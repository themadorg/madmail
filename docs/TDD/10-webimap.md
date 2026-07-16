# WebIMAP & WebSMTP

Madmail-compatible HTTP + WebSocket mail access for web clients and bots. Operator reference: [`context/madmail/docs/chatmail/webimap.md`](../../context/madmail/docs/chatmail/webimap.md). Implementation: `crates/chatmail-www/` (`webimap.rs`, `webimap_ws.rs`, `handlers.rs`).

**Operator CLI:** [`../guide/cli/webimap.md`](../guide/cli/webimap.md) · [`websmtp.md`](../guide/cli/websmtp.md) · [`html-serve.md`](../guide/cli/html-serve.md).

## Feature toggles

Both services are **disabled by default** (Madmail parity). Runtime keys in the settings DB / `passwords` KV:

| Key | Admin resource | Effect when not `"true"` |
|-----|----------------|---------------------------|
| `__WEBIMAP_ENABLED__` | `POST /admin/services/webimap` | All `/webimap/*` REST + `/webimap/ws` return **404** `{"error":"not found"}` |
| `__WEBSMTP_ENABLED__` | `POST /admin/services/websmtp` | `POST /webimap/send`, `POST /websmtp/send`, and WebSocket `send` rejected |

Admin GET/POST use `enable` / `disable` actions (same as other service toggles). `/admin/settings` exposes `webimap_enabled` / `websmtp_enabled` as `"enabled"` / `"disabled"`.

## Authentication

### REST

| Header | Value |
|--------|--------|
| `X-Email` | Full address (`user@domain`) |
| `X-Password` | Account password |

### WebSocket

Query params on upgrade: `?email=USER&password=PASS` (optional `mailbox`, `since_uid`).

CORS (shared with `POST /new`): when browser access is on (WebIMAP **and** WebSMTP enabled), valid request `Origin` values are reflected; optional `__WEBMAIL_CORS_ORIGINS__` whitelist (or `*`). `OPTIONS` → **204** with allowed methods/headers when the origin is allowed.

## REST routes

| Method | Path | Gate | Notes |
|--------|------|------|-------|
| GET | `/webimap/mailboxes` | WebIMAP | INBOX-only maildir; counts from index |
| GET | `/webimap/messages?mailbox=&since_uid=&wait=` | WebIMAP | Long-poll up to `wait` seconds (max 120) |
| GET | `/webimap/message/{uid}?mailbox=` | WebIMAP | Full `MessageDetail` |
| DELETE | `/webimap/message/{uid}` | WebIMAP | Delete by UID |
| DELETE | `/webimap/messages/{mailbox}/{uid}` | WebIMAP | Alias used by `/app` |
| POST | `/webimap/message/flags` | WebIMAP | Flag ops acknowledged (no persistent flags in maildir v1) |
| POST | `/webimap/send` | WebSMTP | JSON `{from,to,body}` — `from` forced to authenticated user |
| POST | `/websmtp/send` | WebSMTP | Legacy alias (same handler) |
| POST | `/new` | — | JIT account creation; JSON `{email, password, dclogin_url}` |
| GET | `/webimap/ws` | WebIMAP | Bidirectional WebSocket (see below) |

## WebSMTP delivery

Shared `websmtp_deliver()` in `handlers.rs` uses the **same authenticated submission
path as SMTP AUTH (587/465)**:

1. `validate_submission_headers` — From/Sender must match authenticated user
2. `enforce_encryption` — PGP-only + SecureJoin handshakes (same as SMTP submission)
3. `DeliveryContext::submit_authenticated` — local maildir **or** outbound federation
   queue (same queue worker as SMTP submission)

Outbound federation (`chatmail-delivery::deliver_remote`) tries HTTPS/HTTP `/mxdeliv`
first, then **always falls back to SMTP :25 / :443** (even when HTTP returns 4xx).
Previously a permanent HTTP 4xx (e.g. missing `/mxdeliv` on the peer) skipped SMTP and
silently dropped WebSMTP/SMTP outbound to peers such as `nine.testrun.org`.

## WebSocket protocol

Envelope (client → server):

```json
{ "req_id": "1", "action": "list_mailboxes", "data": {} }
```

Server reply:

```json
{ "req_id": "1", "action": "result", "data": [ ... ] }
```

| Action | Gate | Status |
|--------|------|--------|
| `list_mailboxes` | WebIMAP | Implemented (INBOX + user folders under `folders/`) |
| `list_messages` | WebIMAP | Implemented |
| `fetch` | WebIMAP | Implemented |
| `delete` | WebIMAP | Implemented |
| `flags` | WebIMAP | Ack only (maildir) |
| `send` | WebSMTP | Implemented |
| `search` | WebIMAP | Implemented (INBOX full-text over subject/from/body/Message-ID) |
| `create_mailbox` / `rename_mailbox` / `delete_mailbox` | WebIMAP | Implemented (maildir subfolders; cannot touch INBOX) |
| `copy` / `move` | WebIMAP | Implemented between INBOX and user folders |

Push (no `req_id`):

```json
{ "action": "new_message", "data": { "uid": 123, "envelope": { ... } } }
```

Poll interval: 2s; also wakes on `AppState` mailbox events.

## Deviations from full IMAP backend (Madmail Go)

- Default delivery is still **INBOX** maildir; user folders live under per-user `folders/`
- SEARCH is a simple substring scan (not full IMAP SEARCH criteria)
- Flags are acknowledged without durable flag persistence (maildir v1)
- REST long-poll uses sleep loop (not IMAP IDLE)

## Testing

Integration tests enable both toggles in `tests/support/mod.rs::spawn_mail_servers`. E2E: `securejoin_e2e`, `p2p` (`/webimap/send`, `/webimap/mailboxes`).

### Unit tests (`crates/chatmail-www/src/tests.rs`)

| Test | Validates |
|------|-----------|
| `new_account_returns_dclogin_url_with_ssl_hints` | `POST /new` returns server-built `dclogin_url` with `ih`/`sh`/`is=ssl`/`ss=ssl` |
| `mail_autoconfig_omits_https_alpn_entry` | Autoconfig route does not emit fake port-443 IMAP entry |
| `connect_host_for_dclogin_prefers_fallback_over_localhost` | Embedded `main.js` skips localhost for dclogin host |

Blocklist checks on WebIMAP auth use `AuthCache::is_blocked` (no DB round-trip).

## Related

- [`03-imap-server.md`](03-imap-server.md) — native IMAP
- [`09-admin-api.md`](09-admin-api.md) — service toggles
- [`12-security.md`](12-security.md) — PGP-only enforcement

## Related RFCs

WebIMAP/WebSMTP mirror IMAP/SMTP semantics over HTTP and WebSocket. Index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local file |
|-----|-------|------------|
| [9110](https://datatracker.ietf.org/doc/html/rfc9110) | HTTP REST routes, methods, status codes | [rfc9110.txt](RFC/rfc9110.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | Mailbox/message model (INBOX, UID, flags) | [rfc3501.txt](RFC/rfc3501.txt) |
| [5322](https://datatracker.ietf.org/doc/html/rfc5322) | Message format on send/fetch | [rfc5322.txt](RFC/rfc5322.txt) |
| [3156](https://datatracker.ietf.org/doc/html/rfc3156) | PGP/MIME on `send` / APPEND-equivalent paths | [rfc3156.txt](RFC/rfc3156.txt) |
| [5321](https://datatracker.ietf.org/doc/html/rfc5321) | WebSMTP submission semantics | [rfc5321.txt](RFC/rfc5321.txt) |
| [8446](https://datatracker.ietf.org/doc/html/rfc8446) | TLS for HTTPS/WSS listeners | [rfc8446.txt](RFC/rfc8446.txt) |
