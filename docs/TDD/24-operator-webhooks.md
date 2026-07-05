# Operator webhooks

Async HTTPS JSON notifications for operator automation (registration, storage quota cap). Configuration is **admin-only** — never exposed on public www pages.

**Crate:** `crates/chatmail-webhooks/`  
**Admin API:** `GET` / `PUT` / `POST` `/admin/services/webhooks`  
**Hooks:** `AppState::webhooks`, `QuotaCache::check_quota`, JIT `/new` / admin account create

---

## Events

| Event | When | Payload fields |
|-------|------|----------------|
| `user.registered` | New account created (JIT, `POST /new`, admin `POST /admin/accounts`) | `username`, `source` (`jit` / `web` / `admin`), `registration_token_used` |
| `user.quota_exceeded` | `QuotaExceeded` on incoming write (deduped per user per hour) | `username`, `used_bytes`, `max_bytes`, `incoming_bytes` |
| `webhook.test` | Admin `POST { "action": "test" }` | `message` |

No mail bodies, passwords, or device tokens in payloads.

---

## Settings (`settings` table)

| Key | Default |
|-----|---------|
| `__WEBHOOK_ENABLED__` | `false` |
| `__WEBHOOK_URL__` | empty |
| `__WEBHOOK_SECRET__` | empty (optional HMAC) |
| `__WEBHOOK_EVENT_USER_REGISTERED__` | `true` |
| `__WEBHOOK_EVENT_QUOTA_EXCEEDED__` | `true` |

URL must be `https://` (or `http://127.0.0.1` / `http://localhost` for tests). When set, requests include `Content-Type: application/json` and optional `X-Madmail-Signature: sha256=<hex>` (HMAC-SHA256 of body).

---

## Admin API

**GET** — config + delivery stats (`successful_deliveries`, `consecutive_failures`). Secret returned only as `secret_configured: true`.

**PUT** — partial update: `enabled`, `url`, `secret`, `event_user_registered`, `event_quota_exceeded`.

**POST** — `{ "action": "test" }` sends a test payload to the configured URL.

Admin-web UI is follow-up work; API is ready for automation and external panels.