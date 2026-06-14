# Ports (`/ports`)

**Source:** `src/routes/ports/+page.svelte`

## Purpose

View and change listener ports for all madmail services, and toggle **public** vs **local-only** access per port.

## Data loaded

| When | Loader | API |
|------|--------|-----|
| Page mount | `store.loadSettings()` | `GET /admin/settings` |
| Prefetch / refresh | Same | Same |

## Port list

Each row edits a setting key via `store.save` / `store.reset`:

| Setting key | Default | Access control |
|-------------|---------|----------------|
| `smtp_port` | 25 | `smtp_access` / `smtp_local_only` |
| `submission_port` | 587 | `submission_access` / `submission_local_only` |
| `submission_tls_port` | 465 | `submission_tls_access` / `submission_tls_local_only` |
| `imap_port` | 143 | `imap_access` / `imap_tls_access` |
| `imap_tls_port` | 993 | `imap_tls_access` / `imap_tls_local_only` |
| `turn_port` | 3478 | `turn_access` / `turn_local_only` |
| `sasl_port` | 24 | `sasl_access` / `sasl_local_only` |
| `iroh_port` | 3340 | `iroh_access` / `iroh_local_only` |
| `ss_port` | 8388 | No access toggle |
| `http_port` | 80 | `http_access` / `http_local_only` |
| `https_port` | 443 | `https_access` / `https_local_only` |

### Change port API

`POST /admin/settings/{port_key}` — `{ "action": "set", "value": "<port>" }`

May return `restart_required: true`.

### Access control API

`store.togglePortAccess(localOnlySetting, currentAccess)`:

| Transition | API |
|------------|-----|
| Public → local | `POST /admin/settings/{service}_local_only` — `{ "action": "set", "value": "true" }` |
| Local → public | `POST /admin/settings/{service}_local_only` — `{ "action": "reset" }` |

Sets `pendingRestart = true` after access changes.

## Confirmation modals

| Scenario | Behavior |
|----------|----------|
| Public → **Local** | Warning modal — local-only binds to loopback; external clients lose access |
| SMTP/IMAP/submission port change | Warning — existing Delta Chat accounts may need reconfiguration |
| HTTP/HTTPS port change | Confirm then save + automatic `POST /admin/reload` (UI reconnects on new port) |

## Typical usage

- Lock SMTP/IMAP to local-only on a machine that only federates outbound
- Change HTTPS port when behind a reverse proxy on a non-standard port
- Reset custom ports to defaults with the reset (↺) button per field