# Federation — Traffic / Peers (`/federation/traffic`)

**Sources:** `src/routes/federation/+layout.svelte`, `src/routes/federation/traffic/+page.svelte`

## Purpose

Inspect per-peer federation delivery statistics: inbound/outbound counts, queue depth, latency, transport breakdown, and quick allow/block actions.

## Data loaded

Inherited from federation layout via `loadFederationSection()`:

| API | Method | Resource |
|-----|--------|----------|
| Peer list | `GET` | `/admin/federation/servers` |
| Rules (for filters) | `GET` | `/admin/federation/rules` |
| Policy | `GET` | `/admin/settings/federation` |

### Health filter URL

Optional query param: `?health=perfect|federated|bad`

- Set by clicking a tier in `FederationStatsGrid` (layout or overview)
- Toggled off when the same tier is clicked again (`federationHealthNav.ts`)
- From overview, navigates to `/federation/traffic?health=…`
- On the traffic tab itself, updates query in place (`replaceState`)

## Server entry fields

| Field | Meaning |
|-------|---------|
| `domain` | Peer identifier (domain or `[ip]`) |
| `inbound_deliveries` | Messages received from peer |
| `successful_deliveries` | Outbound successes |
| `success_http` / `success_https` / `success_smtp` | Transport split |
| `failed_http` / `failed_https` / `failed_smtp` | Delivery failures |
| `queued_messages` | Pending outbound |
| `mean_latency_ms` | Average delivery latency |
| `last_active` | Last activity (unix seconds) |

Local hostnames (`smtp_hostname`, admin URL host) are excluded from the list.

Under **ACCEPT** policy, blocklisted domains with no traffic still appear (synthetic empty entries from rules).

## Filters and sort (client-side)

| Filter | Behavior |
|--------|----------|
| All | No filter |
| Failures | `failed_*` sum > 0 |
| Queued | `queued_messages > 0` |
| Restricted | Domain has federation rule |
| Unrestricted | No rule |
| Health (`?health=`) | Tier from `classifyFederationServer()` |

Sort keys: last active, domain, outbound, inbound, queued, latency, failures.

## Per-peer actions

| Action | API | Body |
|--------|-----|------|
| Block / Allow peer | `POST` | `/admin/federation/rules` — `{ "domain": "..." }` |
| Remove restriction | `DELETE` | `/admin/federation/rules` — `{ "domain": "..." }` |

Button label depends on policy and whether the domain already has a rule.

## Typical usage

- Find peers with high `queued_messages` or failures
- Block abusive domains directly from traffic view
- Use health filter from overview/layout stats to focus on degraded peers
- Correlate HTTPS vs SMTP success for debugging federation paths