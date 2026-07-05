# Admin Web — API Reference

All endpoints are invoked via `POST` to the configured admin API URL with the JSON-RPC envelope described in [README.md](README.md). The `api` object in `src/lib/api.ts` wraps these calls.

## Per-route data loaders

From `src/lib/pageRefresh.ts` — used for prefetch and header **Refresh**:

| Route | Loaders invoked |
|-------|-----------------|
| `/` | `loadOverview({ force: true })` |
| `/services`, `/proxy`, `/ports` | `loadSettings()` |
| `/accounts` | `loadAccountsSection()` + `loadSettings()` |
| `/accounts/blocked`, `/accounts/tokens` | `loadAccountsSection()` |
| `/federation`, `/federation/traffic` | `loadFederationSection()` |
| `/federation/endpoints` | `loadFederationSection()` + `loadEndpointOverrides()` |
| `/federation/exchangers` | `loadFederationSection()` + `loadExchangers()` |
| `/notice` | `loadOverview({ force: true })` |

`loadAccountsSection()` = parallel `accounts`, `blocklist`, `quota`, `registrationTokens`.

`loadFederationSection()` = parallel `federationSettings`, `federationRules`, `federationServers`, `settings`.

## Endpoints by resource

### Dashboard & status

| Resource | Methods used | Called from |
|----------|--------------|-------------|
| `/admin/overview` | GET | Connect, overview, notice |
| `/admin/status` | GET | Connect fallback, auth poll after reload, logo secret |
| `/admin/storage` | GET | Connect fallback (legacy) |
| `/admin/reload` | POST | Header restart, overview soft reload, port/path saves. Body may include `{ "scope": "http", "wait": true }` when remounting admin HTTP routes after `admin_path` change |
| `/admin/restart` | POST | Available in API client; not used in current UI |

### Settings & toggles

| Resource | Methods | Body actions | Pages |
|----------|---------|--------------|-------|
| `/admin/settings` | GET | — | All settings-driven pages |
| `/admin/settings/{key}` | POST | `set`, `reset` | Services, proxy, ports, overview (retention) |
| `/admin/registration` | POST | `open`, `close` | Services |
| `/admin/registration/jit` | POST | `enable`, `disable` | Services |
| `/admin/services/turn` | POST | `enable`, `disable` | Services |
| `/admin/services/iroh` | POST | `enable`, `disable` | Services |
| `/admin/services/push` | POST | `auto`, `disable` | Services, overview |
| `/admin/services/admin_web` | POST | `enable`, `disable` | Services |
| `/admin/services/auto_purge_seen` | POST | `enable`, `disable` | Services |
| `/admin/services/webimap` | POST | `enable`, `disable` | Services |
| `/admin/services/websmtp` | POST | `enable`, `disable` | Services |
| `/admin/services/webhooks` | GET | — | **API only** (no UI yet) |
| `/admin/services/webhooks` | PUT | `{ "enabled"?, "url"?, "secret"?, "event_user_registered"?, "event_quota_exceeded"? }` | **API only** |
| `/admin/services/webhooks` | POST | `{ "action": "test" }` | **API only** |
| `/admin/services/message_retention` | POST | `enable`, `disable` | Overview |
| `/admin/services/shadowsocks` | POST | `enable`, `disable` | Proxy |
| `/admin/services/ss_ws` | POST | `enable`, `disable` | Proxy |
| `/admin/services/ss_grpc` | POST | `enable`, `disable` | Proxy |
| `/admin/services/http_proxy` | POST | `enable`, `disable` | Proxy |
| `/admin/settings/registration_token_required` | POST | `enable`, `disable` | API only (no UI yet) |

**Local-only port settings** (via `POST /admin/settings/{service}_local_only`):  
`smtp_local_only`, `submission_local_only`, `submission_tls_local_only`, `imap_local_only`, `imap_tls_local_only`, `turn_local_only`, `sasl_local_only`, `iroh_local_only`, `http_local_only`, `https_local_only` — Ports page.

### Accounts & access control

| Resource | Methods | Body | Pages |
|----------|---------|------|-------|
| `/admin/accounts` | GET | — | Accounts list |
| `/admin/accounts` | POST | `{}` | Accounts (create) |
| `/admin/accounts` | DELETE | `{ "username" }` | Accounts (delete) |
| `/admin/accounts` | PATCH | `{ "action": "export" }` | Accounts layout |
| `/admin/accounts` | PATCH | `{ "action": "import", "users" }` | Accounts layout |
| `/admin/accounts` | PATCH | `{ "action": "delete_all" }` | Accounts layout |
| `/admin/blocklist` | GET | — | Accounts layout, blocked |
| `/admin/blocklist` | DELETE | `{ "username" }` | Blocked (unblock) |
| `/admin/blocklist` | PATCH | `{ "action": "delete_all" }` | Blocked (unblock all) |
| `/admin/blocklist` | POST | `{ "username", "reason"? }` | **Not used in UI** |
| `/admin/quota` | GET | — | Accounts layout |
| `/admin/quota` | PUT | `{ "max_bytes" }` | Default quota |
| `/admin/quota` | PUT | `{ "username", "max_bytes" }` | Per-user quota |
| `/admin/quota` | DELETE | `{ "username" }` | Reset user quota |
| `/admin/registration-token` | GET | — | Tokens, overview |
| `/admin/registration-token` | POST | `{ "token"?, "max_uses"?, "comment"?, "expires_in"? }` | Tokens |
| `/admin/registration-token` | DELETE | `{ "token" }` | Tokens |

### Federation

| Resource | Methods | Body | Pages |
|----------|---------|------|-------|
| `/admin/settings/federation` | GET | — | Federation layout |
| `/admin/settings/federation` | POST | `{ "enabled"? }` or `{ "policy" }` | Federation layout |
| `/admin/federation/rules` | GET | — | Rules, traffic |
| `/admin/federation/rules` | POST | `{ "domain" }` | Rules, traffic |
| `/admin/federation/rules` | DELETE | `{ "domain" }` | Rules, traffic |
| `/admin/federation/servers` | GET | — | Federation layout, traffic |
| `/admin/dns` | GET | — | Endpoints |
| `/admin/dns` | POST | `{ "lookup_key", "target_host", "comment"? }` | Endpoints |
| `/admin/dns` | DELETE | `{ "lookup_key" }` | Endpoints |
| `/admin/exchangers` | GET | — | Exchangers |
| `/admin/exchangers` | POST | `{ "name", "url", "poll_interval" }` | Exchangers |
| `/admin/exchangers` | PUT | `{ "name", "enabled"? \| "url"? \| "poll_interval"? }` | Exchangers |
| `/admin/exchangers` | DELETE | `{ "name" }` | Exchangers |

### Queue, notices, misc

| Resource | Methods | Body | Pages |
|----------|---------|------|-------|
| `/admin/queue` | POST | `{ "action": "purge_read" \| "purge_all" \| "purge_read_blobs" }` | Overview |
| `/admin/queue` | POST | `{ "action": "purge_blobs" }` | Overview |
| `/admin/queue` | POST | `{ "action": "purge_blobs_older", "retention" }` | Overview |
| `/admin/queue` | POST | `{ "action": "purge_older", "retention" }` | **API client only** (`api.purgeOlder`) — not wired in UI |
| `/admin/notice` | POST | `{ "subject", "body", "recipient"? }` | Notice |

### External (non-madmail)

| URL | Purpose | Page |
|-----|---------|------|
| `https://api.github.com/repos/themadorg/madmail/releases/latest` | Version check | Overview modal |
| `https://github.com/themadorg/madmail/releases/latest` | Download link | Overview modal |

## Setting keys referenced in UI

Grouped by page for quick lookup.

**Services:** `smtp_hostname`, `turn_realm`, `turn_secret`, `turn_relay_ip`, `turn_ttl`, `iroh_relay_url`, `dclogin_imap_security`, `dclogin_smtp_security`, `admin_path`, `admin_web_path`, `language`

**Proxy:** `ss_password`, `ss_port`, `ss_cipher`, `ss_ws_port`, `ss_grpc_port`, `http_proxy_port`, `http_proxy_path`, `http_proxy_username`, `http_proxy_password`, `https_port` (fallback)

**Ports:** `smtp_port`, `submission_port`, `submission_tls_port`, `imap_port`, `imap_tls_port`, `turn_port`, `sasl_port`, `iroh_port`, `ss_port`, `http_port`, `https_port`, plus `*_local_only` access keys

**Overview:** `message_retention`

## API client helpers not exposed in UI

These exist in `src/lib/api.ts` but have no page wired up yet:

- `api.blockUser` → `POST /admin/blocklist`
- `store.toggleTokenRequired` → `POST /admin/settings/registration_token_required`
- `api.restart` → `POST /admin/restart`

Madmail server may also implement resources not used by this SPA (e.g. `/admin/message-size`, `/admin/federation-size`, `/admin/federation/silent-dismiss`, `/admin/shares`). See [`docs/TDD/09-admin-api.md`](../TDD/09-admin-api.md).