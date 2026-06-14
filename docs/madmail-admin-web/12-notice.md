# Notice (`/notice`)

**Source:** `src/routes/notice/+page.svelte`

## Purpose

Send an **admin notice email** to one user or all registered users. Messages are delivered as unencrypted mail to user inboxes (server-side `POST /admin/notice`).

## Data loaded

| When | Loader | API |
|------|--------|-----|
| Page mount | `store.loadOverview()` | `GET /admin/overview` |
| Prefetch / refresh | Same | Same |

Uses `overview.users.registered` for broadcast recipient count.

## Form fields

| Field | Required | Notes |
|-------|----------|-------|
| Mode | Yes | **All users** or **Single recipient** |
| Recipient | Single mode | Email address |
| Subject | Yes | Plain text |
| Body | Yes | Plain text (textarea) |

## Send API

| Mode | API | Body |
|------|-----|------|
| Broadcast | `POST` | `/admin/notice` — `{ "subject", "body", "recipient": "" }` |
| Single | `POST` | `/admin/notice` — `{ "subject", "body", "recipient": "user@domain" }` |

**Response body:** `{ "sent": N, "failed": M, "errors"?: ["..."] }`

Broadcast requires confirmation modal showing total user count.

## Implementation note

The page calls `api.sendNotice()` directly (not via `store`). The API client returns `{ data, error, status }`, but the page currently references `res.body` and `store.showToast` — those identifiers do not exist on the store/API wrapper (`notify` and `data` are the correct ones). Toasts on this page may not work until that is fixed; the inline result panel still shows `sent` / `failed` when `data` is populated.

## Typical usage

- Announce maintenance windows to all users
- Notify a single account about quota or policy issues
- Review `failed` count and per-address errors after send