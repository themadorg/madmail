# Federation — DNS Overrides (`/federation/endpoints`)

**Sources:** `src/routes/federation/+layout.svelte`, `src/routes/federation/endpoints/+page.svelte`

## Purpose

Manage **DNS overrides** (endpoint rewrite table): force federation lookups for a given key to resolve to a specific target host. Used when peers publish wrong DNS or for testing/debugging delivery paths.

## Data loaded

| When | Loader | API |
|------|--------|-----|
| Prefetch on this route | `loadEndpointOverrides()` | `GET /admin/dns` |
| Federation layout | `loadFederationSection()` | (policy + peers) |

## Entry fields

| Field | Meaning |
|-------|---------|
| `lookup_key` | Key looked up during federation (domain or host) |
| `target_host` | Host to use instead |
| `comment` | Optional operator note |

## Actions

| Action | API | Body |
|--------|-----|------|
| Add override | `POST` | `/admin/dns` — `{ "lookup_key", "target_host", "comment"? }` |
| Delete override | `DELETE` | `/admin/dns` — `{ "lookup_key": "..." }` |

Search filters `lookup_key`, `target_host`, and `comment` client-side.

## Typical usage

- Point a peer's federation hostname at a known-good IP when their DNS is broken
- Add temporary overrides during migration between servers
- Document intent in `comment` for other operators