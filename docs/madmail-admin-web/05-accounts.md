# Accounts — List (`/accounts`)

**Sources:** `src/routes/accounts/+layout.svelte` (shared chrome), `src/routes/accounts/+page.svelte` (list)

## Purpose

Manage registered mail accounts: list, search, sort, paginate, set per-user quota, delete, and manually create accounts when registration is closed.

The layout also provides cross-account tools (export/import, default quota, sub-tabs).

## Data loaded

| When | Loader | APIs (parallel) |
|------|--------|-----------------|
| Prefetch / refresh | `loadAccountsSection()` + `loadSettings()` on `/accounts` | See below |

**`loadAccountsSection()` calls:**

| API | Method | Resource |
|-----|--------|----------|
| List accounts | `GET` | `/admin/accounts` |
| Blocklist stats | `GET` | `/admin/blocklist` |
| Quota stats | `GET` | `/admin/quota` |
| Registration tokens | `GET` | `/admin/registration-token` |

## Layout — stats bar

| Stat | Source field |
|------|--------------|
| Total accounts | `accounts.total` |
| Blocked | `blocklist.total` |
| Tokens | `registrationTokens.total` |
| Storage used | `quota.total_storage_bytes` |
| Default quota | `quota.default_quota_bytes` (editable) |

### Layout actions

| Action | API | Body |
|--------|-----|------|
| Export accounts | `PATCH` | `/admin/accounts` — `{ "action": "export" }` → JSON download |
| Import accounts | `PATCH` | `/admin/accounts` — `{ "action": "import", "users": [...] }` |
| Delete all accounts | `PATCH` | `/admin/accounts` — `{ "action": "delete_all" }` |
| Set default quota | `PUT` | `/admin/quota` — `{ "max_bytes": N }` |

## List page — account rows

Each account shows: `username`, `used_bytes`, `max_bytes`, `created_at`, `last_login_at`, `is_default_quota`.

### List actions

| Action | API | Body |
|--------|-----|------|
| Create account | `POST` | `/admin/accounts` — `{}` (server generates email + password) |
| Delete account | `DELETE` | `/admin/accounts` — `{ "username": "..." }` |
| Set user quota | `PUT` | `/admin/quota` — `{ "username": "...", "max_bytes": N }` |
| Reset user quota | `DELETE` | `/admin/quota` — `{ "username": "..." }` |

**Create account** is prominently offered when `settings.registration === "closed"`.

### New account modal

After `POST /admin/accounts`, shows one-time **dclogin:** link built from:

- `smtp_hostname` or admin URL host
- `imap_port`, `submission_port`
- `dclogin_*_security` settings

No API — link is constructed client-side. User must copy before dismissing (password not shown again).

### Client-side features

- Search by username (frontend filter)
- Sort: name, size, created date, last login
- Pagination: 25 / 50 / 100 per page (frontend slice)

## Sub-tabs (layout)

| Tab | Route |
|-----|-------|
| Accounts | `/accounts` |
| Blocked | `/accounts/blocked` |
| Tokens | `/accounts/tokens` |

## Typical usage

- Audit storage per user and bump quota for heavy accounts
- Export JSON backup before maintenance
- Create a test account when public registration is off
- Delete stale accounts after confirming in modal