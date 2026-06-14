# Shell, Login, and Global Navigation

**Source:** `src/routes/+layout.svelte`, `src/routes/+layout.ts`

This is not a separate route, but the authenticated shell and login gate that wrap every page.

## Purpose

- Gate access until a valid admin API URL + bearer token are provided
- Provide top-level navigation between all operator pages
- Handle global UX: theme, locale, PWA updates, server restart banner, route prefetch

## Login gate (unauthenticated)

Shown when `store.connected === false`.

### User inputs

| Field | Stored in | Purpose |
|-------|-----------|---------|
| Admin API URL | `localStorage.madmail_url` | Base URL of JSON-RPC endpoint (e.g. `https://example.com/api/admin`) |
| Admin token | `localStorage.madmail_token` | Bearer token from `admin_token` file or `madmail admin-token` |

### Connect flow

1. User clicks **Connect** (or presses Enter in token field)
2. `store.connect()` → `fetchOverview(config)`
3. On success: credentials saved to `localStorage` and IndexedDB (`saveServer`)
4. Logo animation transitions into the authenticated header

### API on connect

| Call | Method | Resource | Purpose |
|------|--------|----------|---------|
| Primary | `GET` | `/admin/overview` | Auth check + dashboard snapshot |
| Fallback (404) | `GET` | `/admin/status` | Legacy servers without overview |
| Fallback | `GET` | `/admin/storage` | Disk stats |
| Fallback | `GET` | `/admin/registration-token` | Token count |
| Fallback | `GET` | `/admin/settings` | Settings snapshot |

### QR / hash login

- **Scan QR** opens `LoginQrScanner` — user picks a **QR image file** from disk (decoded client-side via `qrDecode.ts`; no live camera stream in current UI)
- Opening the panel URL with a `#base64` fragment (from `madmail admin-token` QR) auto-fills credentials on load (`consumeLoginFromLocation`)
- Legacy query params `madmail_u` / `madmail_t` (or `u` / `t`) are also accepted
- On successful decode: fills URL + token and connects

No API call — credentials are parsed locally from the QR payload.

### Saved servers

- Previously connected servers stored in **IndexedDB** (`src/lib/servers.ts`)
- Click a saved entry to auto-fill and connect
- Delete removes from IndexedDB only

### Certificate errors

If fetch fails with network/CORS errors, UI suggests opening the API URL in a new tab to accept a self-signed certificate.

## Authenticated shell

### Header

| Control | Behavior |
|---------|----------|
| Logo (single tap) | Navigate to `/` |
| Logo (double tap) | Easter-egg “logo secret” overlay (polls `/admin/status` when help activated) |
| Version badge | Shows PWA update dot when a newer build is available |
| **Apply & Restart** | Visible when `store.pendingRestart`; calls `POST /admin/reload` |
| Language picker | en / fa / es / ru via `src/lib/i18n.ts` |
| GitHub link | `themadorg/madmail-admin-web` |
| Theme switcher | Light/dark via `src/lib/stores/theme.svelte.ts` |
| Refresh | `store.refreshForPath(current pathname)` — route-scoped reload |
| Disconnect | Clears session and returns to login gate |

### Navigation tabs

| href | Label key | Page |
|------|-----------|------|
| `/` | `tab.overview` | Dashboard |
| `/services` | `tab.services` | Service toggles + config |
| `/proxy` | `tab.proxy` | Shadowsocks / HTTP proxy |
| `/ports` | `tab.ports` | Listener ports + access |
| `/accounts` | `tab.accounts` | Account management |
| `/federation` | `tab.federation` | Federation policy + sub-tabs |
| `/notice` | `tab.notice` | Admin broadcast email |

### Route prefetch

When connected, navigating to any tab triggers `prefetchRouteData(store, path)` so page data begins loading before the user interacts. See [api-reference.md](api-reference.md) for per-route loader mapping.

### PWA update modal

`src/lib/sw-update.ts` polls `{base}/version.json` every 5 minutes and on tab visibility. Compares against the built-in `__APP_VERSION__`. **Not a madmail API call.**

`applyUpdate()` posts `CLEAR_CACHE` to the service worker and hard-reloads the page.

### Logo easter-egg (double-tap header logo)

Hidden overlay with matrix-rain animation. When the user taps the “help” hotspot inside the overlay, `createLogoSecretStatusPoller` polls **`GET /admin/status`** every 10s and flashes the rain when `sent_messages` increases. Title text adapts based on `serverCapabilities.isMadmailV2` (detected from status shape).

### URL sync after settings change

When connected, `syncConnectionBaseUrlWithSettings()` rewrites the stored admin API URL if hostname/ports/`admin_path` overrides change. In embedded mode, `syncAdminWebPathFromSettings()` redirects the browser if `admin_web_path` no longer matches the current path.

## Usage notes for operators

- First connection requires the **full admin API URL** including path (not just hostname)
- After changing `admin_path` or `admin_web_path` on the server, the SPA may redirect the browser to stay in sync (embedded mode only)
- Remote panels (SPA hosted separately from the server) do not auto-redirect on path changes