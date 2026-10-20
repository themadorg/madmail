# Services (`/services`)

**Source:** `src/routes/services/+page.svelte`

## Purpose

Enable/disable core madmail features and edit server-wide configuration (hostnames, TURN/Iroh, admin paths, Delta Chat login security, frontpage language).

## Deployment protection

Settings follow **environment > explicit config file > database > defaults**.
`configuration_sources` identifies protected settings and their controlling source.
Protected controls are read-only; change their environment/file source and restart
Madmail. CLI/API writes and resets return `configuration_locked`. Removing a
protection exposes the previously stored database value. See the
[configuration guide](../guide/configuration.md).

## Data loaded

| When | Loader | API |
|------|--------|-----|
| Connect (via overview) | Settings embedded in overview | `GET /admin/overview` → `settings` |
| Prefetch / refresh | `store.loadSettings()` | `GET /admin/settings` |

Page renders `PageLoader` until `store.settings` is populated.

## Service toggles

Each row calls `store.toggleService(resource, current)` → `POST` with `enable`/`disable` or `open`/`close` for registration.

| UI label | Resource | On value | Off action |
|----------|----------|----------|------------|
| Registration | `/admin/registration` | `open` | `close` |
| JIT registration | `/admin/registration/jit` | `enabled` | `disable` |
| TURN | `/admin/services/turn` | `enabled` | `disable` |
| Iroh relay | `/admin/services/iroh` | `enabled` | `disable` |
| Push | `/admin/services/push` | runtime on | `disable` / `auto` |
| Admin web UI | `/admin/services/admin_web` | `enabled` | `disable` |
| Auto-purge seen | `/admin/services/auto_purge_seen` | `enabled` | `disable` |
| WebIMAP | `/admin/services/webimap` | `enabled` | `disable` |
| WebSocket mail bridge | `/admin/services/websocket_bridge` | `enabled` | `disable` |
| WebSMTP | `/admin/services/websmtp` | `enabled` | `disable` |

**API body for toggles:** `{ "action": "enable" | "disable" | "open" | "close" | "auto" }`

TURN, Iroh, push, admin_web may set `restart_required` → header shows **Apply & Restart**.

## Configuration fields (editable settings)

Saved via `store.save(key, value)` → `POST /admin/settings/{key}` with `{ "action": "set", "value": "..." }`.

Reset via `store.reset(key)` → `POST /admin/settings/{key}` with `{ "action": "reset" }`.

| Setting key | Purpose |
|-------------|---------|
| `smtp_hostname` | Public SMTP/IMAP hostname |
| `turn_realm` | TURN realm |
| `turn_secret` | TURN shared secret |
| `turn_relay_ip` | TURN relay IP |
| `turn_relay_port_min` | TURN relay UDP port range start (default `49152`) |
| `turn_relay_port_max` | TURN relay UDP port range end (default `65535`) |
| `turn_ttl` | TURN credential TTL (number) |
| `iroh_relay_url` | Iroh relay URL |
| `dclogin_imap_security` | `ssl` / `starttls` / `default` |
| `dclogin_smtp_security` | `ssl` / `starttls` / `default` |
| `admin_path` | Admin API path (triggers HTTP reload + URL update) |
| `admin_web_path` | SPA mount path (may redirect browser) |
| `language` | Frontpage language: `en`, `fa`, `es`, `ru` |

Special UX: `admin_path` and `ss_password` have dice buttons for random values.

**`admin_path` save side-effect:** before switching the stored URL, the client calls `POST /admin/reload` with `{ "scope": "http", "wait": true }`, then polls `GET /admin/status` until the server answers on the new path.

## Typical usage

- Open registration for a public server; close it and use JIT + tokens instead
- Enable TURN/Iroh for Delta Chat calls and large attachments
- Point `smtp_hostname` at the real public name clients should use
- Change `admin_path` / `admin_web_path` to obscure operator endpoints (expect reload)
The WebSocket mail bridge is disabled by default. Its toggle applies to new connections immediately. See [operator/client instructions](../guide/cli/websocket-bridge.md); browser origins must be explicitly allowlisted.
