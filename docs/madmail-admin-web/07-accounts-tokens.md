# Accounts — Registration Tokens (`/accounts/tokens`)

**Sources:** `src/routes/accounts/+layout.svelte`, `src/routes/accounts/tokens/+page.svelte`

## Purpose

Create and manage **registration tokens** used for invite-only account signup (`/inv/{token}` on the public web).

## Data loaded

| API | Method | Resource |
|-----|--------|----------|
| List tokens | `GET` | `/admin/registration-token` |

Loaded via `loadAccountsSection()` with accounts layout data.

## Token entry fields

| Field | Meaning |
|-------|---------|
| `token` | Secret code |
| `max_uses` / `used_count` | Use limit and consumption |
| `pending_reservations` | In-flight reservations |
| `comment` | Operator note |
| `created_at` / `expires_at` | Lifetime |
| `status` | `active`, `exhausted`, or `expired` |

## Actions

| Action | API | Body |
|--------|-----|------|
| Create token | `POST` | `/admin/registration-token` — `{ "token"?, "max_uses"?, "comment"?, "expires_in"? }` |
| Delete token | `DELETE` | `/admin/registration-token` — `{ "token": "..." }` |

Create form fields:

- **Code** — optional (server auto-generates if empty)
- **Max uses** — default 1
- **Comment** — optional label
- **Expires in** — duration string (e.g. `7d`, `24h`) or empty for no expiry

### Client-side helpers (no API)

| Action | Behavior |
|--------|----------|
| Copy token | Clipboard |
| Copy invite link | `https://{host}/inv/{token}` derived from admin URL hostname/port |

> Enabling **registration token required** (`POST /admin/settings/registration_token_required`) is implemented in `store.toggleTokenRequired()` but not exposed in the current UI.

## Typical usage

1. Create tokens with limited uses for onboarding batches
2. Copy invite link and share with users (not the raw admin token)
3. Monitor `exhausted` / `expired` status and delete old tokens
4. Search/filter when many tokens exist