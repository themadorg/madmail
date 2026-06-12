# Phase P1 — Implementation plan (index)

Cross-repo plan: **madmail-v2** (server, done) + **Delta Chat Core** + **deltachat-desktop** (client, this phase).

## Problem statement

Madmail and madmail-v2 expose **WebIMAP** (HTTP + WebSocket) and **WebSMTP** so clients without native IMAP can send/receive mail. The Rust server in `madmailv2/crates/chatmail-www/` already matches Madmail behaviour. Delta Chat desktop and core still use **only async-IMAP and SMTP**.

The madweb / deltachat-web-mono stack proves the protocol in TypeScript (`transport.ts`). P1 ports that client into Core and exposes a **user-visible experimental toggle** on desktop — same UX pattern as `webxdc_realtime_enabled`.

## Server inventory (madmailv2 — no P1 server work required)

| Piece | Location |
|-------|----------|
| REST handlers | `crates/chatmail-www/src/webimap.rs` |
| WebSocket | `crates/chatmail-www/src/webimap_ws.rs` |
| Send + auth | `crates/chatmail-www/src/handlers.rs` (`webimap_authenticate`, `websmtp_deliver`) |
| Routes | `crates/chatmail-www/src/router.rs` |
| Feature gates | `crates/chatmail-www/src/gate.rs` (`__WEBIMAP_ENABLED__`, `__WEBSMTP_ENABLED__`) |
| CLI | `chatmail webimap enable|disable|status`, `websmtp …` |
| Admin | `POST /admin/services/webimap`, `/admin/services/websmtp` |
| Integration tests | `crates/chatmail-www/src/tests.rs`, `tests/securejoin_e2e.rs`, `tests/support/mod.rs` |

Operator must enable both services; disabled → **404** `{"error":"not found"}` (Madmail parity).

## Core inventory (context/core)

| Area | Path | Notes |
|------|------|-------|
| IMAP client | `src/imap.rs`, `src/imap/` | Production receive/sync |
| SMTP send | `src/smtp.rs`, scheduler | Production send |
| Transports table | `src/transport.rs` | Host/port/security candidates |
| Chatmail detect | `src/context.rs` `is_chatmail()` | Gate experimental transport |
| Test harness | `src/tests/chatmail_transport.rs` | Spawn `chatmail`, `/new`, REST mailboxes only |
| Config pattern | `webxdc_realtime_enabled` in `src/config.rs` | Copy for new bool |
| Connectivity UI data | `src/scheduler/connectivity.rs` | Extend with WebIMAP status |

## Desktop inventory (context/deltachat-desktop)

| Area | Path | Notes |
|------|------|-------|
| Advanced settings | `packages/frontend/src/components/Settings/Advanced.tsx` | Insert new section |
| Toggle pattern | `WebxdcRealtime.tsx` | `getConfig` / `setConfig` via RPC |
| Transports dialog | `TransportsDialog` | Unchanged; WebIMAP uses HTTP base URL from login |

## HTTP base URL derivation

WebIMAP lives on the **HTTP(S) listener**, not IMAP port.

| Source | Rule |
|--------|------|
| Chatmail QR / configure | Often same host as `mail_host`; use **HTTPS** on 443 or configured `CHATMAIL_HTTP_ADDR` from provider metadata if present |
| Manual | New optional config `webimap_base_url` (e.g. `https://nine.testrun.org`) when auto-detect fails |
| Dev | `http://127.0.0.1:{CHATMAIL_HTTP_ADDR}` from test subprocess |

P1-S01 defines precedence: explicit config → provider entry → `https://{configured_addr}` with certificate checks from existing transport settings.

## Hybrid scheduler behaviour (P1 default)

When `webimap_transport_enabled=1` **and** `is_chatmail()` **and** probe says server WebIMAP+WebSMTP enabled:

1. **Connect:** open `wss://…/webimap/ws?email=…&password=…&mailbox=INBOX&since_uid={last}` (password from configured login param).
2. **Push:** on `new_message`, `fetch` full MIME, pass to existing `receive_imf`/ingest (same as IMAP FETCH body).
3. **Poll fallback:** if WS drops, use REST long-poll `GET /webimap/messages?wait=60` or reconnect WS (SDK pattern).
4. **Send:** if outbound job ready, `send` WS action or `POST /webimap/send` with PGP MIME body (same validation as SMTP).
5. **Parallel IMAP:** keep periodic IMAP connect for MVBOX, quota, metadata, moves — **less frequent** when WS healthy (connectivity shows "WebIMAP active").

When toggle off or server 404: **unchanged** IMAP/SMTP only.

## Security & privacy

| Topic | Mitigation in P1 |
|-------|------------------|
| Password in WS URL query | Document; prefer WSS; optional `webimap_ws_use_headers` **future** if server adds header auth on upgrade |
| Experimental default | **Off** (`webimap_transport_enabled` default `0`) |
| Non-chatmail accounts | Toggle hidden / ignored |
| Server disabled | Probe returns 404 → show connectivity warning, fall back to IMAP |

## Dependency graph

```
P1-S01 config & eligibility
    ├── P1-S02 types
    │     ├── P1-S03 REST
    │     └── P1-S04 WebSocket
    │           ├── P1-S05 receive
    │           └── P1-S06 send
    ├── P1-S07 connectivity probe
    ├── P1-S08 desktop UI (needs S01 config key)
    └── P1-S09 E2E (needs S03–S06)
P1-S10 operator runbook (parallel)
```

## CI gates (by step)

| Step | Tests to run before merge |
|------|---------------------------|
| S01 | `p1_ut00`, `p1_ut00b` |
| S02 | `p1_ut01`, `p1_ut01b` |
| S03 | `p1_ut02`, `p1_ut02b`, `cargo test -p chatmail-www` |
| S04 | `p1_ut03`, `p1_ut03b`, `p1_ut04` |
| S05 | `p1_ut05`, `p1_it02`, `p1_e2e01` (e2e optional until S09) |
| S06 | `p1_ut06`, `p1_it03`, `p1_e2e02` |
| S07 | `p1_ut07`, `p1_ut08`, `p1_e2e03` |
| S08 | P1-UI01 manual checklist |
| S09 | `./scripts/core-e2e-webimap.sh` (all `p1_e2e*`) |
| S10 | `p1_it04`, `p1_it05`, `cargo test -p chatmail-www` |

```bash
# Full phase (after all steps)
cd madmailv2 && ./scripts/core-e2e-webimap.sh
cargo test -p chatmail-www
cargo test -p chatmail-integration securejoin
```

## Related phases

| Phase | Relationship |
|-------|----------------|
| b5–b6 | IMAP server Core depends on for hybrid |
| b9 | Same cross-repo pattern (core config + desktop + subprocess tests) |
| Future P2 | Full folder parity, drop hybrid IMAP |
