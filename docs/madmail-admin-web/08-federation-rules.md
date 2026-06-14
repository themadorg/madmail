# Federation — Rules (`/federation`)

**Sources:** `src/routes/federation/+layout.svelte`, `src/routes/federation/+page.svelte`

## Purpose

Control federation policy (ACCEPT vs REJECT), enable/disable federation globally, and manage per-domain **exception rules**.

The layout (`+layout.svelte`) is shared across all federation sub-routes and shows policy controls + aggregate stats.

## Data loaded

**`loadFederationSection()`** (prefetch / refresh on any `/federation/*` route):

| API | Method | Resource |
|-----|--------|----------|
| Federation settings | `GET` | `/admin/settings/federation` |
| Rules list | `GET` | `/admin/federation/rules` |
| Peer stats | `GET` | `/admin/federation/servers` |
| Settings (hostname) | `GET` | `/admin/settings` |

## Layout — policy header

| Control | API | Body |
|---------|-----|------|
| Federation on/off | `POST` | `/admin/settings/federation` — `{ "enabled": true \| false }` |
| Switch ACCEPT ↔ REJECT | `POST` | `/admin/settings/federation` — `{ "policy": "ACCEPT" \| "REJECT" }` |

### Policy semantics

| Policy | Rules list meaning |
|--------|-------------------|
| **ACCEPT** (open) | Rules are a **blocklist** — listed domains are denied |
| **REJECT** (closed) | Rules are an **allowlist** — only listed domains are permitted |

`FederationStatsGrid` in the layout aggregates peer health from `federationServers` (excluding local hostnames). Clicking a tier filters the Traffic tab via `?health=` query param.

When federation is disabled, an inactive banner is shown; rules UI remains accessible.

## Rules tab (`/federation`)

### List

| Field | Source |
|-------|--------|
| Domain | `rules[].domain` (bracketed IP literals normalized in display) |
| Created | `rules[].created_at` (unix seconds) |

Search and pagination (20 per page) are client-side.

### Actions

| Action | API | Body |
|--------|-----|------|
| Add domain | `POST` | `/admin/federation/rules` — `{ "domain": "example.org" }` |
| Delete rule | `DELETE` | `/admin/federation/rules` — `{ "domain": "..." }` |

Add button label changes with policy: **Block** under ACCEPT, **Allow** under REJECT.

## Federation sub-tabs

| Tab | Route | Doc |
|-----|-------|-----|
| Rules | `/federation` | This page |
| Peers / Traffic | `/federation/traffic` | [09-federation-traffic.md](09-federation-traffic.md) |
| DNS overrides | `/federation/endpoints` | [10-federation-endpoints.md](10-federation-endpoints.md) |
| Exchangers | `/federation/exchangers` | [11-federation-exchangers.md](11-federation-exchangers.md) |

## Typical usage

- Start with ACCEPT + blocklist entries for known bad peers
- Or REJECT + allowlist for a private federation circle
- Toggle federation off during maintenance without deleting rules