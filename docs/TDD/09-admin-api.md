# Admin API Design

Madmail-compatible JSON-RPC admin API. Full operator reference: [`context/madmail/docs/chatmail/admin_api.md`](../../context/madmail/docs/chatmail/admin_api.md). Implementation: `crates/chatmail-admin/`, wired from `chatmail-fed` HTTP listener.

**CLI equivalents:** many admin resources have `madmail` subcommands — see [`../guide/cli/README.md`](../guide/cli/README.md) and the mapping table below. TDD parity: [14-cli-tools.md](14-cli-tools.md).

## Design goals

1. **Single endpoint** — `POST {admin_path}` (default `/api/admin`)
2. **JSON-RPC envelope** — `method`, `resource`, `headers`, `body`
3. **Bearer token** — `{state_dir}/admin_token` (0600), constant-time compare
4. **HTTP 200 always** — real status in JSON `status` field (anti-enumeration)
5. **Rate limit** — 10 failed auth attempts / minute / IP
6. **1 MB** request body cap (before auth)
7. **No secrets in responses** — passwords never returned

## Request / response

```json
{
  "method": "GET",
  "resource": "/admin/status",
  "headers": { "Authorization": "Bearer <token>" },
  "body": {}
}
```

```json
{
  "status": 200,
  "resource": "/admin/status",
  "body": { },
  "error": null,
  "version": "0.1.0"
}
```

## Resource catalogue (Madmail parity)

| Resource | Methods | Status in madmail-v2 |
|----------|---------|------------------------|
| `/admin/status` | GET | Implemented (live IMAP session count + `ss` fallback on `__IMAP_PORT__` / `__IMAP_TLS_PORT__`). Legacy; prefer `/admin/overview` for the admin-web dashboard. |
| `/admin/overview` | GET | Implemented — dashboard summary: status metrics, host `disk`, registration `tokens.total`, and full `settings` snapshot (one call for admin-web overview) |
| `/admin/storage` | GET | Implemented (`disk` via statvfs, `state_dir`, `database`) |
| `/admin/restart` | POST | Stub (logs only; no systemd) |
| `/admin/reload` | POST | **Soft reload** — stop SMTP/IMAP/HTTP, `AppState::hydrate`, rebind listeners from DB ports (admin-web “Apply & Restart”) |
| `/admin/registration` | GET, POST | Implemented |
| `/admin/registration/jit` | GET, POST | Implemented |
| `/admin/services/turn` | GET, POST | `__TURN_ENABLED__`; POST triggers soft reload (embedded webrtc-rs TURN + IMAP TURN metadata) |
| `/admin/services/iroh` | GET, POST | `__IROH_ENABLED__` (default on when configured); POST triggers soft reload (embedded iroh-relay v0.35.0 + IMAP `/shared/vendor/deltachat/irohrelay`) |
| `/admin/services/admin_web` | GET, POST | DB toggle only |
| `/admin/services/auto_purge_seen` | GET, POST | Implemented (`__AUTO_PURGE_SEEN__`, default disabled) |
| `/admin/services/webimap` | GET, POST | Implemented (`__WEBIMAP_ENABLED__`, default disabled) |
| `/admin/services/websmtp` | GET, POST | Implemented (`__WEBSMTP_ENABLED__`, default disabled) |
| `/admin/services/push` | GET, POST | Implemented — `__PUSH_MODE__` (`auto`/`on`/`off`, **default `off`**); POSTs device tokens to `notifications.delta.chat` when enabled; GET returns `successful_notifications`, `consecutive_failures`; POST `enable`/`disable`/`auto` → soft reload. See [23-push-notifications.md](23-push-notifications.md) |
| `/admin/services/webhooks` | GET, PUT, POST | Implemented — operator HTTPS webhooks for `user.registered` and `user.quota_exceeded`; POST `{ "action": "test" }`. See [24-operator-webhooks.md](24-operator-webhooks.md) |
| `/admin/settings/federation` | GET, POST | Implemented — includes `max_federation_size`, `federation_size_effective` |
| `/admin/federation/rules` | GET, POST, DELETE | Implemented |
| `/admin/federation/silent-dismiss` | GET, POST, DELETE | Implemented — outbound domains accepted but not delivered (`federation_silent_dismiss` table) |
| `/admin/federation/servers` | GET | Implemented (`FederationTracker`) |
| `/admin/accounts` | GET, DELETE | Implemented |
| `/admin/blocklist` | GET, POST, DELETE | Implemented |
| `/admin/quota` | GET, PUT, DELETE | Implemented |
| `/admin/dns` | GET, POST, DELETE | Implemented (`dns_overrides`) |
| `/admin/exchangers` | GET, POST, PUT, DELETE | Implemented |
| `/admin/settings` | GET | Implemented (Madmail `AllSettingsResponse` shape) |
| `/admin/settings/*` | GET, POST | Implemented (ports, paths, language, security, …) |
| `/admin/notice` | GET, POST | Implemented (unencrypted admin email to inbox) |
| `/admin/queue` | POST | Implemented (maildir purge + `purge_queue` for outbound retry dir) |
| `/admin/shares` | * | Not yet (CLI `madmail sharing` + `sharing.db` implemented; see [17-data-models.md](17-data-models.md)) |
| `/admin/services/shadowsocks` | GET, POST | **Implemented** when `ss_addr` + `ss_password` in `maddy.conf`; toggle via `__SS_ENABLED__`; 400 when SS not configured |
| `/admin/services/ss_ws` | GET, POST | Always `disabled` — raw TCP only; `enable` returns 400 |
| `/admin/services/ss_grpc` | GET, POST | Always `disabled` — raw TCP only; `enable` returns 400 |
| `/admin/services/http_proxy` | GET, POST | Stub — not implemented |
| `/admin/settings/ss_port`, `ss_cipher`, `ss_password`, … | GET, POST | Implemented when SS configured; `ss_ws_*` / `ss_grpc_*` settings stored but transports disabled |
| `/admin/settings/http_proxy_*` | GET, POST | Stub — changes return 400 |
| `/admin/message-size` | GET, PUT, DELETE | Implemented — effective cap (`appendlimit` ∧ `max_message_size`) |
| `/admin/federation-size` | GET, PUT, DELETE | Implemented — `/mxdeliv` HTTP body cap (default **70M**); PUT/DELETE trigger HTTP routes reload |
| `/admin/registration-token` | GET, POST, DELETE | Implemented — registration token CRUD |

Toggle POST body: `{"action": "enable"}` or `{"action": "disable"}`.

Push POST body (`/admin/services/push`): `{"action": "auto"}` | `"enable"` / `"on"` | `"disable"` / `"off"` — see [23-push-notifications.md](23-push-notifications.md). Admin-web toggle uses `auto` (on) and `disable` (off).

Setting POST body: `{"action": "set", "value": "..."}` or `{"action": "reset"}`.

### Push in status / overview

`GET /admin/status` and `GET /admin/overview` include:

```json
"push": {
  "enabled": false,
  "mode": "off",
  "successful_notifications": 0,
  "consecutive_failures": 0,
  "auto_disable_after": 5
}
```

`GET /admin/settings` adds `push_mode` and legacy `push_enabled` for admin-web.

### `/admin/notice` (Madmail `resources/notice.go`)

Operator broadcast: deliver a **plain-text, unencrypted** RFC 5322 message into each recipient’s local maildir (same path as SMTP local delivery; no PGP / encryption enforcement).

| Method | Body | Response |
|--------|------|----------|
| GET | — | `{ "total_users": N, "domain": "example.com" }` — `domain` from first account or recipient |
| POST | `{ "subject", "body", "recipient" }` | `{ "sent", "failed", "errors"? }` |

- `recipient` empty → all accounts from `passwords` (excludes `__*` KV keys).
- `recipient` set → single user; localpart-only values get `@domain` appended.
- Sender: `Admin <admin@domain>`; per-recipient delivery (partial failures still HTTP 200 unless **all** fail → 500).
- Reference: `context/madmail/internal/api/admin/resources/notice.go`, admin-web `sendNotice()` in `admin-web/src/lib/api.ts`.

### `/admin/queue` (Madmail `resources/queue.go`)

Two storage areas:

1. **User maildir** (`{state_dir}/mail/`) — Madmail `state_dir/messages/` + IMAP SQL; madmail-v2 uses maildir files.
2. **Outbound retry queue** (`{state_dir}/remote_queue/`) — Madmail `target.queue`; failed federation deliveries are retried from disk (see [07-federation.md](07-federation.md)).

| `action` | Body fields | Effect |
|----------|-------------|--------|
| `purge_blobs` | — | Delete all files in all users’ `new/`, `cur/`, `tmp/` |
| `purge_blobs_older` | `retention` (e.g. `1h`, `72h`) | Delete message files older than retention (mtime) |
| `purge_user` | `username` | Delete one user’s maildir message files |
| `purge_all` | — | Same as `purge_blobs` (all users) |
| `purge_read` / `purge_read_blobs` | — | Delete `cur/` only (seen/opened in maildir) |
| `purge_older` | `retention` | Delete `new/` files older than retention (unread prune) |
| `purge_queue` | — | Delete all entries in `{state_dir}/remote_queue/` |

Response shape: `{ "action", "message", "deleted"?: N }`.

## Authentication

- Token file: `admin_token` in state dir (64 hex chars)
- Config: `admin_token disabled` in `chatmail` block → API off
- Config: `admin_path` / `__ADMIN_PATH__` (default `/api/admin`)

## Implementation layout (Rust)

```
crates/chatmail-admin/
  src/handler.rs    # RPC dispatch, envelope (HTTP 200 + JSON status)
  src/auth.rs       # Bearer + rate limit
  src/cors.rs       # CORS for admin API
  src/router.rs     # AdminState + axum POST /
  src/resources/    # accounts, blocklist, dns, exchangers, federation, federation_size,
                    # message_size,
                    # notice, proxy, push, queue, quota, settings, status_storage,
                    # toggles, tokens

crates/chatmail/src/servers.rs
  build_admin_router() → nest under admin_path on HTTP listener
crates/chatmail-fed/src/server.rs
  run_http_listener(..., extra: Option<Router>) merges /mxdeliv + admin
```

## Tests

| ID | Scope |
|----|--------|
| `p9_admin_status_get` | GET `/admin/status` |
| `p9_federation_rules_crud` | federation rules POST/GET/DELETE |
| `p9_blocklist_post_get` | blocklist POST/GET |
| `p9_auto_purge_seen_toggle` | `/admin/services/auto_purge_seen` enable/disable + settings sync |
| `p9_status_message_counters` | Live atomic counters in `/admin/status` |
| `p9_shadowsocks_not_configured` | SS toggle returns 400 when not in `maddy.conf` |
| `p9_shadowsocks_configured_toggle` | SS GET/POST toggle when `ss_addr` + `ss_password` set |
| `p9_ss_ws_and_grpc_transports_disabled` | WS/gRPC SS transports always disabled |
| `p9_federation_silent_dismiss_crud` | `/admin/federation/silent-dismiss` CRUD |
| `admin_message_size_get_put_delete` | `/admin/message-size` effective cap |
| `admin_federation_size_get_put_delete` | `/admin/federation-size` effective cap (default 70M) |
| `admin_federation_settings_includes_size` | `GET /admin/settings/federation` exposes federation size |
| `admin_settings_max_federation_size_updates_effective` | `POST /admin/settings/max_federation_size` |
| `p9_auth_gate_bearer` | constant-time Bearer check |
| `p9_notice_post_delivers` | POST `/admin/notice` → local maildir |
| `p9_queue_purge_blobs_older` | POST `/admin/queue` `purge_blobs_older` |
| `p9_push_service_toggle` | GET/POST `/admin/services/push` (mode + stats) |
| `p9_status_push_stats` | `push` object in `/admin/status` |

Run: `cargo test -p chatmail-admin`

## Public web UI (`www`)

Madmail embeds `internal/endpoint/chatmail/www/` as the main site (index, docs, `/new`, `/qr`, static CSS/JS). madmail-v2 serves the same tree from `crates/chatmail-www` (source: `www-src/`, build-time, `rust-embed`).

| Path | Purpose |
|------|---------|
| `/` | Registration landing (`index.html`) |
| `/new` | POST JSON account creation |
| `/qr` | QR PNG for `dclogin:` links |
| `/docs/` | Operator documentation |
| `/share` | Contact share form |
| `/app` | Delta Chat web client shell |

Mounted on the HTTP listener together with `/mxdeliv` and `/api/admin` (see `crates/chatmail/src/servers.rs`).

## Web admin panel (Svelte)

Madmail serves a separate SPA from `admin-web/` via `adminweb.go`. madmail-v2 embeds **`external/madmail-admin-web`** via `chatmail-admin-web` on the HTTP listener (same origin as `/api/admin`).

Push UI: overview card + services row — toggle (`auto`/`disable`), successful-notification count, `notifications.delta.chat` copy. See [23-push-notifications.md](23-push-notifications.md#admin-web-embedded-spa).

## Admin API ↔ CLI mapping

| Admin resource / setting | CLI command | Guide |
|--------------------------|-------------|-------|
| `/admin/reload` | `madmail reload` | [reload.md](../guide/cli/reload.md) |
| `/admin/registration` | `madmail registration` | [registration.md](../guide/cli/registration.md) |
| `/admin/federation/rules` | `madmail federation` | [federation.md](../guide/cli/federation.md) |
| `/admin/federation/silent-dismiss` | `madmail federation dismiss` | [federation.md](../guide/cli/federation.md) |
| `/admin/registration-token` | `madmail registration-tokens` | [registration-tokens.md](../guide/cli/registration-tokens.md) |
| `/admin/dns` | `madmail endpoint-cache` | [endpoint-cache.md](../guide/cli/endpoint-cache.md) |
| `/admin/blocklist` | `madmail blocklist` | [blocklist.md](../guide/cli/blocklist.md) |
| `/admin/accounts` | `madmail accounts` | [accounts.md](../guide/cli/accounts.md) |
| `/admin/services/push` | `madmail push` | [push.md](../guide/cli/push.md) |
| `/admin/services/webimap` | `madmail webimap` | [webimap.md](../guide/cli/webimap.md) |
| `/admin/services/websmtp` | `madmail websmtp` | [websmtp.md](../guide/cli/websmtp.md) |
| `/admin/services/admin_web` | `madmail admin-web` | [admin-web.md](../guide/cli/admin-web.md) |
| `/admin/settings/*` ports | `madmail port` | [port.md](../guide/cli/port.md) |
| Message size settings | `madmail message-size` | [message-size.md](../guide/cli/message-size.md) |
| `/admin/queue` purge | `madmail tasks run` | [tasks-run.md](../guide/cli/tasks-run.md) |
| Bearer token | `madmail admin-token` | [admin-token.md](../guide/cli/admin-token.md) |

Use `--json` on CLI for machine-readable output ([`json-output.md`](../guide/cli/json-output.md)).

## Implementation references

| Concern | Madmail |
|---------|---------|
| RPC router | [`internal/api/admin/admin.go`](../../context/madmail/internal/api/admin/admin.go) |
| Resources | [`internal/api/admin/resources/`](../../context/madmail/internal/api/admin/resources/) |
| Registration | [`chatmail.go` setupAdminAPI](../../context/madmail/internal/endpoint/chatmail/chatmail.go) |
| Admin web | [`adminweb.go`](../../context/madmail/internal/endpoint/chatmail/adminweb.go) |
| Settings keys | [`settings.go`](../../context/madmail/internal/api/admin/resources/settings.go) |

## Related RFCs

Admin API is HTTP + JSON over a single endpoint. Offline copies: [`RFC/README.md`](RFC/README.md). Regenerate: [`RFC/download-rfcs.sh`](RFC/download-rfcs.sh).

| RFC | Topic | Local file |
|-----|-------|------------|
| [9110](https://datatracker.ietf.org/doc/html/rfc9110) | HTTP semantics | [rfc9110.txt](RFC/rfc9110.txt) |
| [8259](https://datatracker.ietf.org/doc/html/rfc8259) | JSON bodies | [rfc8259.txt](RFC/rfc8259.txt) |
| [6750](https://datatracker.ietf.org/doc/html/rfc6750) | Bearer token pattern | [rfc6750.txt](RFC/rfc6750.txt) |
