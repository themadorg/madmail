# Push notifications (XDELTAPUSH)

Delta Chat mobile wake-up via IMAP `SETMETADATA` device tokens and the central notification proxy at `https://notifications.delta.chat/notify`.

**Crate:** `crates/chatmail-push/` (`notifier`, `store`, `mode`, `enabled`, `stats`)  
**IMAP:** `crates/chatmail-imap/` (`XDELTAPUSH`, `METADATA`, `/private/devicetoken`)  
**Admin:** `/admin/services/push`, `push` block in `/admin/status` and `/admin/overview`  
**CLI:** `madmail push` — guide: [`../guide/cli/push.md`](../guide/cli/push.md) (binary name is **`madmail`**, not `chatmail`)

Reference: Dovecot/chatmaild `notifier.py`, cmdeploy `XDELTAPUSH` capability.

**Advertisement (#120):** When push is enabled, `XDELTAPUSH` and `METADATA` appear only on **post-auth** IMAP `CAPABILITY`. Pre-auth responses stay generic so unauthenticated probes do not fingerprint the relay.

---

## Flow

1. Delta Chat registers an encrypted device token over IMAP:
   ```
   SETMETADATA INBOX (/private/devicetoken "openpgp:…")
   ```
2. Token is stored in SQLite `push_tokens` (per user, multiple devices).
3. On **inbound local delivery** (not self-sent), `AppState::notify_inbound_push()` queues one notification job per token.
4. `PushNotifier` POSTs the **raw token string** as the HTTP body to the notify URL (no JSON, no mail content).
5. HTTP **2xx** → success counter + reset consecutive-failure counter. **410 Gone** → remove stale token.

### Inbound paths that trigger push

| Path | Crate |
|------|-------|
| SMTP local delivery | `chatmail-smtp` |
| Federation HTTP `/mxdeliv` | `chatmail-fed` |
| WebSMTP `route_message` | `chatmail-delivery` |
| Admin notice | `chatmail-admin` |

---

## Notification proxy request

```
POST https://notifications.delta.chat/notify
Content-Length: <len(token)>

<device_token>
```

- Per-request timeout: **20 seconds** (slow or failed requests count toward auto-disable).
- Retries: exponential backoff (8s base) up to 24 attempts; jobs persist under `{state_dir}/pending_notifications/` for crash recovery.

---

## Settings (`settings` table)

| Key | Values | Default |
|-----|--------|---------|
| `__PUSH_MODE__` | `auto`, `on`, `off` | **`off`** (seeded on install) |
| `__PUSH_ENABLED__` | `true` / `false` | **`false`** by default; kept in sync for legacy admin builds |

### Modes

| Mode | Runtime push | Auto-disable |
|------|--------------|--------------|
| **off** | Off | **Default** — no POSTs to `notifications.delta.chat` |
| **auto** | On | **Yes** — after **5 consecutive** failed device-token deliveries, mode is set to **off** |
| **on** | On | No |

A **failed delivery** (one device token job that gives up): HTTP error, network/timeout (>20s), exhausted retries, 410 Gone, or 5-hour job deadline. A **success** resets the consecutive failure counter.

When auto mode trips, the server logs an error and suggests `madmail push auto` or `madmail push on`.

---

## Admin API

### `GET /admin/services/push`

```json
{
  "status": "enabled",
  "mode": "auto",
  "successful_notifications": 42,
  "consecutive_failures": 0,
  "auto_disable_after": 5
}
```

### `POST /admin/services/push`

| `action` | Effect |
|----------|--------|
| `enable` / `on` | Mode **on** |
| `disable` / `off` | Mode **off** |
| `auto` | Mode **auto** |

Response includes `restart_required: true` (soft reload refreshes post-auth IMAP `XDELTAPUSH` advertisement).

### Status / overview

`GET /admin/status` and `GET /admin/overview` include a `push` object with `enabled`, `mode`, `successful_notifications`, `consecutive_failures`, `auto_disable_after`.

Settings bundle adds `push_mode` (`auto` | `on` | `off`).

### Admin web (embedded SPA)

Source: `external/madmail-admin-web/` (embedded via `chatmail-admin-web`).

| Location | UI |
|----------|-----|
| Overview (`/`) | Push card at bottom — toggle, successful-notification count, `notifications.delta.chat` hint |
| Services (`/services`) | Push row with same toggle and endpoint label |

- **Default off** — card shows off hint; no `XDELTAPUSH` until enabled.
- **Toggle on** → `POST /admin/services/push` with `action: "auto"` (circuit breaker).
- **Toggle off** → `action: "disable"`.
- Stats from `GET /admin/overview` → `push.successful_notifications`; older servers without `push` in status show “unsupported”.

---

## CLI

```bash
madmail push status   # mode, runtime, success count, failure count
madmail push auto     # enable with auto-disable circuit breaker
madmail push on       # force on (no circuit breaker)
madmail push off      # default — no POSTs to notifications.delta.chat
```

Per-subcommand docs: [`push-status.md`](../guide/cli/push-status.md), [`push-auto.md`](../guide/cli/push-auto.md), [`push-on.md`](../guide/cli/push-on.md), [`push-off.md`](../guide/cli/push-off.md).

After changing mode, run **`madmail reload`** ([`reload.md`](../guide/cli/reload.md)) so IMAP capabilities match the DB.

Implementation: `crates/chatmail/src/ctl/push.rs`, `chatmail-config::cli::PushCommand`.

---

## Metrics

Successful deliveries increment `push_successful_notifications` in the `message_stats` table (in-memory counter + 30s flush), exposed in admin status/overview.

---

## Tests

| ID | Scope |
|----|--------|
| `imap_e2e_push_devicetoken_setmetadata` | Pre-auth omits push caps; post-auth SET/GET METADATA round-trip |
| `imap_e2e_push_disabled_hides_capabilities` | Push off → no post-auth `XDELTAPUSH` |
| `p9_push_service_toggle` | Admin GET/POST `/admin/services/push` |
| `p9_status_push_stats` | `push` block in `/admin/status` |
| `push_mode_and_circuit_breaker` | Auto trip after 5 failures (`chatmail-push`) |
| `successful_delivery_increments_push_stats` | Wiremock notify + counter |
| `setmetadata-devicetoken` | `tools/relay-ping` IMAP check |

Run: `cargo test -p chatmail-push -p chatmail-admin`, `cargo test -p chatmail-integration imap_e2e_push`

---

## Privacy / persistence

- **No per-message DB writes** — inbound delivery only reads `push_tokens` and enqueues disk-backed notify jobs.
- **HTTP body** is the raw device token only (no mail content, username, or subject).
- **Stats** — success counter is in-memory with 30s flush to `message_stats`; mode changes persist to `settings` on auto-trip or admin/CLI toggle.

---

## Related docs

- [03-imap-server.md](03-imap-server.md) — `XDELTAPUSH`, METADATA
- [09-admin-api.md](09-admin-api.md) — resource catalogue + admin-web
- [13-configuration.md](13-configuration.md) — `__PUSH_MODE__` settings keys
- [14-cli-tools.md](14-cli-tools.md) — `madmail push`
- [17-data-models.md](17-data-models.md) — `push_tokens` table