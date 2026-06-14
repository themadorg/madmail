# Accounts — Blocked (`/accounts/blocked`)

**Sources:** `src/routes/accounts/+layout.svelte`, `src/routes/accounts/blocked/+page.svelte`

## Purpose

View accounts on the server blocklist and unblock individuals or everyone at once.

Blocking **new** users is not done from this page (use server CLI or API `POST /admin/blocklist` — the SPA exposes `api.blockUser` in `api.ts` but no UI route calls it yet).

## Data loaded

Same as accounts section — `loadAccountsSection()` on prefetch/refresh:

| API | Method | Resource |
|-----|--------|----------|
| Blocklist | `GET` | `/admin/blocklist` |

Also loads accounts, quota, tokens for the layout stats bar.

## UI

### Header

- Total blocked count
- **Unblock all** (when count > 0)

### Search

Frontend filter on `username` and `reason`.

### Per-entry display

| Field | Source |
|-------|--------|
| Username | `blocked[].username` |
| Reason | `blocked[].reason` |
| Blocked at | `blocked[].blocked_at` (ISO timestamp) |

### Actions

| Action | API | Body |
|--------|-----|------|
| Unblock one | `DELETE` | `/admin/blocklist` — `{ "username": "..." }` |
| Unblock all | `PATCH` | `/admin/blocklist` — `{ "action": "delete_all" }` |

Unblock one requires confirmation modal.

## Typical usage

- Review why accounts were blocked (`reason` field)
- Restore access after a false positive
- Clear entire blocklist after migration (with care — confirm dialog on unblock all)