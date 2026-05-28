# WebIMAP desktop handoff (for testing with colleague)

## What was fixed in Core

1. **WebSocket TLS** — WSS now uses the same certificate/SPKI logic as IMAP and HTTP (fixes `UnknownIssuer` while REST probe was OK).
2. **Sent tick (first ✓)** — After successful WebSMTP send, Core calls `set_delivered()` like normal SMTP.

## RPC package (already built on this machine)

```bash
cd desktop/core/deltachat-rpc-server/npm-package
python scripts/make_local_dev_version.py
```

Desktop links `@deltachat/stdio-rpc-server` to:

`relay/madmailv2/context/core/deltachat-rpc-server/npm-package`

(same tree as `desktop/core`).

## Run desktop

```bash
cd desktop/deltachat-desktop
pnpm install   # if needed
pnpm start     # or your usual dev command
```

**Fully quit** Electron before restarting so the new RPC binary loads.

## Account settings

1. Advanced → **WebIMAP transport** → enable.
2. If host is an IP or non-standard HTTPS URL, set **`webimap_base_url`** (e.g. `https://mail.example.com`).
3. Toggle WebIMAP **off → on** once after upgrade (restarts scheduler + WS loop).

## Connectivity checks

| Field | Expected |
|-------|----------|
| Server WebIMAP probe | reachable |
| WebSocket | **connected** (not disconnected) |
| Last error | empty |

## Send / receive checks

| Check | Expected |
|-------|----------|
| Send message | **First tick** (delivered) appears |
| Other user reads | **Second tick** when they send read receipt (needs their client + your receive path) |

## Core-only tests (no desktop)

```bash
cd madmailv2
./scripts/core-e2e-webimap.sh
```

Remote server:

```bash
WEBIMAP_REMOTE_TEST=1 \
WEBIMAP_TEST_BASE_URL=https://mail.example.com \
WEBIMAP_TEST_EMAIL=you@example.com \
WEBIMAP_TEST_PASSWORD=secret \
./scripts/core-e2e-webimap.sh remote
```

## Server side (operator)

WebIMAP + WebSMTP must be enabled on chatmail (`__WEBIMAP_ENABLED__`, `__WEBSMTP_ENABLED__`).

## WebXDC Iroh line

Unrelated to WebIMAP — “no relay on chatmail” is expected unless admin enables TURN/Iroh on the server.
