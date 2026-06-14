# Federation — Exchangers (`/federation/exchangers`)

**Sources:** `src/routes/federation/+layout.svelte`, `src/routes/federation/exchangers/+page.svelte`

## Purpose

Configure **exchanger** feeds — HTTP endpoints polled periodically to discover federation peers or routing hints (chatmail exchanger protocol).

## Data loaded

| When | Loader | API |
|------|--------|-----|
| Prefetch on this route | `loadExchangers()` | `GET /admin/exchangers` |
| Federation layout | `loadFederationSection()` | (policy + peers) |

## Entry fields

| Field | Meaning |
|-------|---------|
| `name` | Unique exchanger id |
| `url` | Poll URL |
| `enabled` | Whether polling is active |
| `poll_interval` | Seconds between polls |
| `last_poll_at` | ISO timestamp of last successful poll |

## Actions

| Action | API | Body |
|--------|-----|------|
| Add exchanger | `POST` | `/admin/exchangers` — `{ "name", "url", "poll_interval" }` |
| Enable/disable | `PUT` | `/admin/exchangers` — `{ "name", "enabled": true \| false }` |
| Update poll interval | `PUT` | `/admin/exchangers` — `{ "name", "poll_interval": N }` |
| Delete | `DELETE` | `/admin/exchangers` — `{ "name": "..." }` |

Poll interval is editable inline (click the interval label).

## Typical usage

- Register with a public exchanger directory to advertise your server
- Poll partner exchangers to auto-discover peers in a community
- Disable an exchanger temporarily without deleting configuration
- Watch `last_poll_at` to detect connectivity issues to the feed URL