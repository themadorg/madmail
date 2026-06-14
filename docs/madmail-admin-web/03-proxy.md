# Proxy (`/proxy`)

**Source:** `src/routes/proxy/+page.svelte`

## Purpose

Configure circumvention/proxy transports: Shadowsocks (raw TCP), WebSocket and gRPC SS transports, and HTTP CONNECT proxy. Displays ready-to-copy client URLs and QR codes.

## Data loaded

| When | Loader | API |
|------|--------|-----|
| Page mount | `store.loadSettings()` | `GET /admin/settings` |
| Prefetch / refresh | Same | Same |

## Sections

### Shadowsocks (raw TCP)

| Action | API | Notes |
|--------|-----|-------|
| Enable/disable | `POST /admin/services/shadowsocks` | `{ "action": "enable" \| "disable" }` |
| Set password | `POST /admin/settings/ss_password` | Random generator in UI |
| Set port | `POST /admin/settings/ss_port` | Default 8388 |
| Set cipher | `POST /admin/settings/ss_cipher` | e.g. `chacha20-ietf-poly1305` |
| Reset any field | `POST /admin/settings/{key}` | `{ "action": "reset" }` |

**Client URL:** Built client-side in `store.shadowsocksUrl` from hostname (admin URL), cipher, password, port — or taken from `settings.shadowsocks_url` when server provides it. Copy + QR (`ShadowsocksQR` component).

Toggling SS sets `pendingRestart` (deferred until **Apply & Restart**).

### WebSocket transport

| Action | API |
|--------|-----|
| Toggle | `POST /admin/services/ss_ws` |
| Port | `POST /admin/settings/ss_ws_port` |

### gRPC+TLS transport

| Action | API |
|--------|-----|
| Toggle | `POST /admin/services/ss_grpc` |
| Port | `POST /admin/settings/ss_grpc_port` |

> On current madmail builds, `ss_ws` and `ss_grpc` may always report `disabled` at the API level (raw TCP only). The UI still exposes controls for forward compatibility.

### HTTP CONNECT proxy

| Action | API |
|--------|-----|
| Toggle | `POST /admin/services/http_proxy` |
| Port | `POST /admin/settings/http_proxy_port` |
| Path | `POST /admin/settings/http_proxy_path` |
| Username | `POST /admin/settings/http_proxy_username` |
| Password | `POST /admin/settings/http_proxy_password` |

**Connection string:** Derived in UI as  
`https://{user}:{pass}@{host}:{port}{path}`  
when password is set. Uses admin URL hostname and falls back to `https_port` or 443.

> HTTP proxy service may be stubbed on the server (`400` on enable). UI remains for parity with madmail-admin-web upstream.

### Restart banner

When `store.pendingRestart`, shows inline **Apply & Restart** → `POST /admin/reload`.

## Typical usage

1. Enable Shadowsocks, set a strong password, copy `ss://` URL into Delta Chat or a client
2. Adjust ports if 8388 is blocked
3. Apply & Restart after toggling transports
4. HTTP proxy section for environments that need CONNECT through HTTPS (when server supports it)