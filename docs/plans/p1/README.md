# Phase P1 — WebIMAP / WebSMTP in Delta Chat Core + Desktop

## Goal

Let **Delta Chat Core** (and **deltachat-desktop**) optionally use the madmail-v2 **WebIMAP REST + WebSocket** and **WebSMTP** stack instead of native async-IMAP/SMTP — behind an **experimental per-account toggle**, mirroring the existing `webxdc_realtime_enabled` pattern.

**Server side:** already implemented in `crates/chatmail-www/` ([TDD 10-webimap](../../TDD/10-webimap.md)). Operators enable with `chatmail webimap enable` / `websmtp enable` or Admin API.

**Client side (this phase):** Rust WebSocket/REST client in Core, scheduler hooks, Advanced-settings switch in desktop.

## Why this is a separate phase

| Layer | Status |
|-------|--------|
| madmail-v2 HTTP `/webimap/*`, `/webimap/ws`, `/websmtp/send` | **Done** (`webimap.rs`, `webimap_ws.rs`, `handlers.rs`) |
| Madmail / web SDK reference | **Done** (`context/madmail/docs/chatmail/webimap.md`, `desktop/deltachat-web-mono/packages/sdk/lib/transport.ts`, `desktop/protocol/websocket_spec.md`) |
| Delta Chat Core production path | **IMAP + SMTP only** today |
| Core test helper | **REST only** — `http_get_mailboxes` in `context/core/src/tests/chatmail_transport.rs` |
| deltachat-desktop UI | **No toggle** yet |

WebIMAP was designed for **browser clients** without raw IMAP. Desktop already has IMAP; P1 is an **opt-in experiment** (push latency, restricted networks, parity with madweb) — not a default replacement.

## Capability gaps (must read before coding)

From [10-webimap.md](../../TDD/10-webimap.md) vs full Core IMAP usage ([03-imap-server.md](../../TDD/03-imap-server.md) § Delta Chat client):

| Core needs | WebIMAP today | P1 handling |
|------------|---------------|-------------|
| INBOX + MVBOX / chats folder | INBOX only | **Block toggle** or force IMAP for multi-folder until storage grows |
| IDLE push | WS `new_message` + 2s poll | Map push → `idle_interrupted` |
| UID FETCH bodies | `fetch` / REST message | Map UID → existing ingest |
| MOVE / COPY / flags | Not supported (maildir v1) | Keep IMAP for outbound housekeeping when hybrid |
| SMTP submission | `send` action / REST | Route send when `websmtp` enabled |
| METADATA / QUOTA / COMPRESS | N/A | Stay on IMAP for those even in hybrid v1 |

**Recommended P1 mode:** **Hybrid** — WebSocket for **receive push + optional send**, IMAP for folder sync / moves / metadata (same account, two channels). Full replacement is **out of scope** for P1.

## Architecture (target)

```
┌──────────────────────── deltachat-desktop ────────────────────────┐
│  Advanced → "WebIMAP transport (experimental)"                     │
│       │ setConfig("webimap_transport_enabled", "1"|"0")           │
└───────┼───────────────────────────────────────────────────────────┘
        │ JSON-RPC
┌───────▼────────────────── context/core ───────────────────────────┐
│  Config::WebimapTransportEnabled                                    │
│  webtransport/          ← new module (Rust port of SDK transport)   │
│    rest.rs              GET/POST + X-Email / X-Password             │
│    ws.rs                /webimap/ws JSON-RPC + push                  │
│  scheduler              when enabled && is_chatmail():              │
│    receive: ws push → fetch UID via REST/ws → existing MIME ingest  │
│    send:    websmtp send (PGP MIME body) if enabled                   │
│    else:    existing Imap::connect / smtp                           │
└───────┼─────────────────────────────────────────────────────────────┘
        │ HTTPS / WSS (configured HTTP host, port 443/80)
┌───────▼────────────────── madmail-v2 ──────────────────────────────┐
│  chatmail-www: /webimap/*  gated by __WEBIMAP_ENABLED__             │
│                websmtp     gated by __WEBSMTP_ENABLED__               │
└─────────────────────────────────────────────────────────────────────┘
```

## TDD / reference index

| Doc | Role |
|-----|------|
| [10-webimap.md](../../TDD/10-webimap.md) | Normative server API (madmailv2) |
| [03-imap-server.md](../../TDD/03-imap-server.md) | Core IMAP command surface |
| [09-admin-api.md](../../TDD/09-admin-api.md) | Operator service toggles |
| [16-testing.md](../../TDD/16-testing.md) | E2E philosophy |
| `context/madmail/docs/chatmail/webimap.md` | Operator + protocol detail |
| `desktop/protocol/websocket_spec.md` | Client protocol narrative |
| `desktop/deltachat-web-mono/packages/sdk/lib/transport.ts` | **Reference client** to port |

## Prerequisites

- madmail-v2 Phases 4–5 (SMTP submission + IMAP) for hybrid fallback and parity tests
- Account configured as **chatmail** (`Context::is_chatmail()`)
- HTTP listener reachable at same host users use for `/new` (from `ConfiguredLoginParam` or provider DB)
- Server: `__WEBIMAP_ENABLED__` and `__WEBSMTP_ENABLED__` = `"true"` (tests: `tests/support/mod.rs`)

## Test matrix (one block per step — phase done when all green)

| ID | Tier | Step | What it proves | Command |
|----|------|------|----------------|---------|
| **P1-UT00** | Unit | S01 | Config default off; get/set round-trip | `cd context/core && cargo test p1_ut00` |
| **P1-UT00b** | Unit | S01 | Eligibility: chatmail on, MVBOX blocks | `cargo test p1_ut00b` |
| **P1-UT01** | Unit | S02 | `WsRequest` / `WsResponse` serde | `cargo test p1_ut01` |
| **P1-UT01b** | Unit | S02 | `new_message` push JSON parses | `cargo test p1_ut01b` |
| **P1-UT02** | Unit | S03 | REST auth headers + URL build | `cargo test p1_ut02` |
| **P1-UT02b** | Unit | S03 | REST JSON → `MessageSummary` (fixtures) | `cargo test p1_ut02b` |
| **P1-IT01** | Integration | S03 | chatmail-www handlers unchanged | `cargo test -p chatmail-www` |
| **P1-UT03** | Unit | S04 | WS `req_id` correlate request/response | `cargo test p1_ut03` |
| **P1-UT03b** | Unit | S04 | Push without `req_id` → channel | `cargo test p1_ut03b` |
| **P1-UT04** | Unit | S04 | Reconnect clears pending; backoff capped | `cargo test p1_ut04` |
| **P1-UT05** | Unit | S05 | UID cursor updated after ingest | `cargo test p1_ut05` |
| **P1-IT02** | Integration | S05 | Push handler calls ingest with MIME fixture | `cargo test p1_it02` |
| **P1-UT06** | Unit | S06 | SMTP-style errors mapped from HTTP status | `cargo test p1_ut06` |
| **P1-IT03** | Integration | S06 | WS `send` accepted when websmtp on (mock) | `cargo test p1_it03` |
| **P1-UT07** | Unit | S07 | Probe: 404 when webimap disabled | `cargo test p1_ut07` |
| **P1-UT08** | Unit | S07 | Connectivity HTML lists WS state | `cargo test p1_ut08` |
| **P1-UI01** | Manual | S08 | Desktop toggle persists config | See [P1-S08](P1-S08-desktop-toggle.md) |
| **P1-E2E01** | E2E | S09 | Receive mail over WS into Core DB | `CHATMAIL_WEBIMAP_TEST=1 cargo test p1_e2e01` |
| **P1-E2E02** | E2E | S09 | P2P send via WebSMTP / WS | `CHATMAIL_WEBIMAP_TEST=1 cargo test p1_e2e02` |
| **P1-E2E03** | E2E | S09 | Probe 200 after `webimap enable` | `CHATMAIL_WEBIMAP_TEST=1 cargo test p1_e2e03` |
| **P1-IT04** | Integration | S10 | Server CLI toggles DB keys | `cargo test -p chatmail p1_it04` |
| **P1-IT05** | Integration | S10 | securejoin + webimap e2e regression | `cargo test -p chatmail-integration securejoin` |

All Core tests run from `context/core` (or path in your checkout). Wrapper:

```bash
./scripts/core-e2e-webimap.sh   # builds chatmail, sets CHATMAIL_BIN + CHATMAIL_WEBIMAP_TEST=1
```

## Steps

| Step | File | Summary | Tests |
|------|------|---------|-------|
| P1-S01 | [P1-S01-gap-analysis-config.md](P1-S01-gap-analysis-config.md) | Config keys, eligibility | P1-UT00, P1-UT00b |
| P1-S02 | [P1-S02-core-protocol-types.md](P1-S02-core-protocol-types.md) | JSON types | P1-UT01, P1-UT01b |
| P1-S03 | [P1-S03-rest-client.md](P1-S03-rest-client.md) | REST client | P1-UT02, P1-UT02b, P1-IT01 |
| P1-S04 | [P1-S04-websocket-client.md](P1-S04-websocket-client.md) | WebSocket client | P1-UT03, P1-UT03b, P1-UT04 |
| P1-S05 | [P1-S05-scheduler-receive.md](P1-S05-scheduler-receive.md) | Receive path | P1-UT05, P1-IT02, P1-E2E01 |
| P1-S06 | [P1-S06-scheduler-send.md](P1-S06-scheduler-send.md) | Send path | P1-UT06, P1-IT03, P1-E2E02 |
| P1-S07 | [P1-S07-connectivity-probe.md](P1-S07-connectivity-probe.md) | Probe + connectivity | P1-UT07, P1-UT08, P1-E2E03 |
| P1-S08 | [P1-S08-desktop-toggle.md](P1-S08-desktop-toggle.md) | Desktop UI | P1-UI01 |
| P1-S09 | [P1-S09-e2e-core-chatmail.md](P1-S09-e2e-core-chatmail.md) | Core ↔ chatmail subprocess | P1-E2E01–03 |
| P1-S10 | [P1-S10-server-operator-runbook.md](P1-S10-server-operator-runbook.md) | Operator runbook | P1-IT04, P1-IT05 |

## Overview document

[phase-p1-implementation-plan.md](phase-p1-implementation-plan.md)

## Out of scope (follow-up P2+)

- Replace IMAP entirely (MVBOX, MOVE, SEARCH, multi-folder)
- Browser `target-browser` using Core WebIMAP (stays on JSON-RPC to core)
- Auto-enable server flags from desktop (operator-only)
- TLS client cert / OAuth for WebIMAP (password in query string remains spec)
