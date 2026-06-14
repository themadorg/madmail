# Overview (`/`)

**Source:** `src/routes/+page.svelte`

## Purpose

Server health dashboard: at-a-glance metrics, disk usage, federation traffic summary, maintenance actions, push toggle, and madmail version check.

Primary audience: operators who want status without drilling into sub-pages.

## Data loaded

| When | Loader | API |
|------|--------|-----|
| Connect | `fetchOverview` | `GET /admin/overview` (or legacy bundle) |
| Navigate back to `/` | `store.loadOverview({ force: true })` | Same |
| Header refresh | `loadOverview({ force: true })` | Same |

Overview data is held in `store.overview` (type `OverviewResponse`).

## UI sections

### Stat cards (bento grid)

| Card | Data field | Links to |
|------|------------|----------|
| Registered users | `overview.users.registered` | `/accounts` |
| Uptime | `overview.uptime.duration` | — |
| Registration tokens | `overview.tokens.total` | `/accounts/tokens` |
| Sent messages | `overview.sent_messages` | — |
| Outbound messages | `overview.outbound_messages` | — |
| Received messages | `overview.received_messages` | — |
| Madmail version | `overview.version` | Opens update modal |
| IMAP connections | `overview.imap` | — |
| TURN relays | `overview.turn.relays` | — |
| Shadowsocks connections | `overview.shadowsocks` | `/proxy` |

### Shadowsocks client URL

Shown when `store.shadowsocksUrl` is non-empty (derived from settings when SS is enabled). Copy and QR display; no extra API call.

### Disk usage bar

From `overview.disk`: `used_bytes`, `available_bytes`, `total_bytes`, `percent_used`.

### Federation traffic

When `overview.federation_traffic` exists, shows `FederationStatsGrid` (inbound/outbound/queued/expired/latency/health tiers). Clicking a health tier navigates to `/federation/traffic?health=...`.

Also shows peer counts from `overview.email_servers` (connections, domain_servers, ip_servers).

### Maintenance — message files

| Action | API | Body |
|--------|-----|------|
| Toggle message retention | `POST /admin/services/message_retention` | `{ "action": "enable" \| "disable" }` |
| Set retention days | `POST /admin/settings/message_retention` | `{ "action": "set", "value": "30d" }` |
| Purge blobs older than | `POST /admin/queue` | `{ "action": "purge_blobs_older", "retention": "72h" }` |
| Purge read blobs | `POST /admin/queue` | `{ "action": "purge_read_blobs" }` |
| Purge all message files | `POST /admin/queue` | `{ "action": "purge_blobs" }` |

Retention options: `1h`, `6h`, `24h`, `72h`, `168h`, `720h`. Day options: 1, 3, 7, 14, 30, 90.

### Maintenance — server actions

| Action | API | Body |
|--------|-----|------|
| Soft reload | `POST /admin/reload` | `{}` or `{ "scope": "full" }` |
| Purge all queue | `POST /admin/queue` | `{ "action": "purge_all" }` |
| Purge read queue | `POST /admin/queue` | `{ "action": "purge_read" }` |

### Push notifications

| Action | API | Body |
|--------|-----|------|
| Toggle push | `POST /admin/services/push` | `{ "action": "auto" \| "disable" }` (enable uses `auto`) |

Displays `overview.push.successful_notifications` when push API is available.

### Version modal

| Action | External | Purpose |
|--------|----------|---------|
| Check for updates | `GET https://api.github.com/repos/themadorg/madmail/releases/latest` | Compare with running version |
| Open releases | Browser → GitHub releases | Download binaries |

No madmail API call for update check.

## Typical usage

1. Connect and scan overview cards for anomalies (disk >70%, federation health, queue depth)
2. Use quick links to accounts or federation traffic
3. Run targeted purges or toggle retention/push without leaving the dashboard
4. Soft-reload after changes that set `pendingRestart` on other pages