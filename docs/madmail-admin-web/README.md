# Madmail Admin Web — Page Documentation

This folder documents every page of the operator dashboard SPA in [`external/madmail-admin-web`](../../external/madmail-admin-web) (Git submodule: `themadorg/madmail-admin-web`).

The app is a **SvelteKit** single-page application embedded into the madmail binary at build time (`make build-with-admin-web`) and served under the configured `admin_web_path` (default `/admin`). It has **no backend of its own** — all data and mutations go through the madmail **JSON-RPC admin API** (`POST {admin_path}`, default `/api/admin`).

## Navigation map

| Route | Tab label | Doc |
|-------|-----------|-----|
| `/` | Overview | [01-overview.md](01-overview.md) |
| `/services` | Services | [02-services.md](02-services.md) |
| `/proxy` | Proxy | [03-proxy.md](03-proxy.md) |
| `/ports` | Ports | [04-ports.md](04-ports.md) |
| `/accounts` | Accounts (list) | [05-accounts.md](05-accounts.md) |
| `/accounts/blocked` | Accounts → Blocked | [06-accounts-blocked.md](06-accounts-blocked.md) |
| `/accounts/tokens` | Accounts → Tokens | [07-accounts-tokens.md](07-accounts-tokens.md) |
| `/federation` | Federation → Rules | [08-federation-rules.md](08-federation-rules.md) |
| `/federation/traffic` | Federation → Peers | [09-federation-traffic.md](09-federation-traffic.md) |
| `/federation/endpoints` | Federation → DNS | [10-federation-endpoints.md](10-federation-endpoints.md) |
| `/federation/exchangers` | Federation → Exchangers | [11-federation-exchangers.md](11-federation-exchangers.md) |
| `/notice` | Notice | [12-notice.md](12-notice.md) |

Shared shell (login gate, header, nav, refresh): [00-shell-and-auth.md](00-shell-and-auth.md).

Full API catalogue used by the SPA: [api-reference.md](api-reference.md).

## How the SPA talks to madmail

Every call uses `apiCall()` in `src/lib/api.ts`:

```http
POST {admin_api_url}
Content-Type: application/json

{
  "method": "GET|POST|PUT|DELETE|PATCH",
  "resource": "/admin/...",
  "headers": { "Authorization": "Bearer <token>" },
  "body": { ... }
}
```

Responses are always parsed as JSON with `{ status, resource, body, error, version }`. The UI treats `error` as failure regardless of HTTP status.

### Deployment modes

| Mode | API URL | Notes |
|------|---------|-------|
| Embedded | Same origin as SPA (e.g. `https://host/admin` → API at `https://host/api/admin`) | `admin_web_path` redirects handled automatically |
| Remote panel | User-entered URL (e.g. hosted at `admin.madmail.chat` pointing at `https://1.2.3.4/api/admin`) | CORS / self-signed cert: open API URL in browser first |
| Electron | Requests proxied via `POST /__proxy` on `127.0.0.1` | Avoids CORS and cert issues |
| Vite dev | Same-origin path with `VITE_DEV_API_PROXY=1` | Proxied to `DEV_API_PROXY_TARGET` |

### Data loading strategy

On connect, `fetchOverview()` loads dashboard data in one request (`GET /admin/overview`) when the server supports it; otherwise it composes legacy calls in parallel (`/admin/status`, `/admin/storage`, `/admin/registration-token`, `/admin/settings`).

Per-route prefetch and the header **Refresh** button use `src/lib/pageRefresh.ts` to load only what the current page needs.

### Global actions (available from header on any connected page)

| Action | API |
|--------|-----|
| Refresh current route | Route-specific loaders (see each page doc) |
| Apply & Restart | `POST /admin/reload` |
| Disconnect | Clears `localStorage` credentials |

## Source layout (relevant files)

```
external/madmail-admin-web/
├── src/routes/           # SvelteKit pages (one +page.svelte per route)
├── src/lib/api.ts        # Admin API client + TypeScript types
├── src/lib/state.svelte.ts  # Shared reactive store (all mutations)
└── src/lib/pageRefresh.ts   # Per-route prefetch / refresh loaders
```

Backend resource implementations: `crates/chatmail-admin/src/resources/`. TDD reference: [`docs/TDD/09-admin-api.md`](../TDD/09-admin-api.md).

## Coverage audit

### Routes (complete)

All **12 SvelteKit routes** under `src/routes/` are documented. There are no hidden or dynamic routes beyond the federation/accounts sub-paths listed above.

| Route file | Documented |
|------------|------------|
| `+layout.svelte` | [00-shell-and-auth.md](00-shell-and-auth.md) |
| `+page.svelte` (overview) | [01-overview.md](01-overview.md) |
| `services/+page.svelte` | [02-services.md](02-services.md) |
| `proxy/+page.svelte` | [03-proxy.md](03-proxy.md) |
| `ports/+page.svelte` | [04-ports.md](04-ports.md) |
| `accounts/+layout.svelte` | [05-accounts.md](05-accounts.md) |
| `accounts/+page.svelte` | [05-accounts.md](05-accounts.md) |
| `accounts/blocked/+page.svelte` | [06-accounts-blocked.md](06-accounts-blocked.md) |
| `accounts/tokens/+page.svelte` | [07-accounts-tokens.md](07-accounts-tokens.md) |
| `federation/+layout.svelte` | [08-federation-rules.md](08-federation-rules.md) |
| `federation/+page.svelte` | [08-federation-rules.md](08-federation-rules.md) |
| `federation/traffic/+page.svelte` | [09-federation-traffic.md](09-federation-traffic.md) |
| `federation/endpoints/+page.svelte` | [10-federation-endpoints.md](10-federation-endpoints.md) |
| `federation/exchangers/+page.svelte` | [11-federation-exchangers.md](11-federation-exchangers.md) |
| `notice/+page.svelte` | [12-notice.md](12-notice.md) |

### Non-page features (in shell doc)

These are not separate routes but are part of the app:

- Logo easter-egg overlay (double-tap header logo) — polls `GET /admin/status`
- PWA self-update checker — fetches `{base}/version.json` (not madmail API)
- QR / hash login — image-file QR decode + URL `#base64` hash (no API)
- IndexedDB saved servers — local only
- Theme switcher + i18n (en/fa/es/ru) — local only
- Federation health deep-link — `?health=perfect|federated|bad` on traffic tab

### API client methods with no UI (documented in api-reference)

`blockUser`, `getToggle`, `getSetting`, `purgeOlder` (`purge_older`), `restart`, `store.toggleTokenRequired`, `store.refresh()` (full reload — header uses scoped `refreshForPath` instead).

### Madmail admin API resources not used by this SPA

The server may expose endpoints the dashboard does not call yet, including:

- `GET/PUT/DELETE /admin/message-size`
- `GET/PUT/DELETE /admin/federation-size`
- `/admin/federation/silent-dismiss`
- `/admin/shares`
- `GET /admin/notice` (SPA only POSTs)

Settings present in `AllSettings` but with **no editable UI**: `log_disabled`, `registration_token_required` (toggle exists in `state.svelte.ts` only).