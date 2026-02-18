# Settings Database

The `settings` table is a key-value store for server-wide configuration flags and values.
It is backed by a SQL table and supports dynamic updates without server restarts.

## Toggle Settings (boolean flags)

These settings control feature toggles. They store `"true"` or `"false"` as string values.

| Key | Default | Description |
|-----|---------|-------------|
| `__REGISTRATION_OPEN__` | `false` | Whether new user registration is open |
| `__JIT_REGISTRATION_ENABLED__` | same as registration | Whether Just-In-Time registration on login is enabled |
| `__TURN_ENABLED__` | `true` | Whether the TURN relay service is enabled |
| `__LOG_DISABLED__` | `false` | Whether logging is suppressed (No Log Policy) |
| `__IROH_ENABLED__` | `true` | Whether the Iroh relay service is enabled |
| `__SS_ENABLED__` | `true` | Whether the Shadowsocks proxy service is enabled |

## Port Settings

These settings allow overriding the port numbers for each service endpoint.
When not set, the port from the configuration file is used.

| Key | Description |
|-----|-------------|
| `__SMTP_PORT__` | SMTP server port (default: from config, typically 25) |
| `__SUBMISSION_PORT__` | Submission server port (default: from config, typically 587/465) |
| `__IMAP_PORT__` | IMAP server port (default: from config, typically 993) |
| `__TURN_PORT__` | TURN relay server port |
| `__DOVECOT_PORT__` | Dovecot SASL authentication port |
| `__IROH_PORT__` | Iroh relay port |
| `__SS_PORT__` | Shadowsocks proxy port |

## Configuration Settings

These settings allow overriding other endpoint configuration values dynamically.

| Key | Description |
|-----|-------------|
| `__SMTP_HOSTNAME__` | SMTP server hostname |
| `__TURN_REALM__` | TURN server realm |
| `__TURN_SECRET__` | TURN server shared secret |
| `__TURN_RELAY_IP__` | TURN server relay IP address |
| `__TURN_TTL__` | TURN credential TTL (seconds) |
| `__IROH_RELAY_URL__` | Iroh relay URL |
| `__SS_CIPHER__` | Shadowsocks cipher algorithm |
| `__SS_PASSWORD__` | Shadowsocks password |

## Admin API Endpoints

All settings are managed through the single-endpoint RPC-style Admin API at `POST /api/admin`.

### Bulk Read

Get all settings at once:

```json
{
    "method": "GET",
    "resource": "/admin/settings",
    "headers": { "Authorization": "Bearer <token>" }
}
```

### Toggle Settings

Toggle a boolean setting (registration, turn, iroh, shadowsocks, log):

```json
{
    "method": "POST",
    "resource": "/admin/registration",
    "headers": { "Authorization": "Bearer <token>" },
    "body": { "action": "open" }
}
```

Actions: `open`/`close` for registration, `enable`/`disable` for services.

### Value Settings

Set a port or configuration value:

```json
{
    "method": "POST",
    "resource": "/admin/settings/smtp_port",
    "headers": { "Authorization": "Bearer <token>" },
    "body": { "action": "set", "value": "2525" }
}
```

Reset to default (remove from DB):

```json
{
    "method": "POST",
    "resource": "/admin/settings/smtp_port",
    "headers": { "Authorization": "Bearer <token>" },
    "body": { "action": "reset" }
}
```

### Available Setting Endpoints

| Endpoint | Type | Description |
|----------|------|-------------|
| `/admin/settings` | GET | Bulk read all settings |
| `/admin/registration` | GET/POST | Registration toggle |
| `/admin/registration/jit` | GET/POST | JIT registration toggle |
| `/admin/services/turn` | GET/POST | TURN service toggle |
| `/admin/services/iroh` | GET/POST | Iroh service toggle |
| `/admin/services/shadowsocks` | GET/POST | Shadowsocks service toggle |
| `/admin/services/log` | GET/POST | Logging toggle |
| `/admin/settings/smtp_port` | GET/POST | SMTP port |
| `/admin/settings/submission_port` | GET/POST | Submission port |
| `/admin/settings/imap_port` | GET/POST | IMAP port |
| `/admin/settings/turn_port` | GET/POST | TURN port |
| `/admin/settings/dovecot_port` | GET/POST | Dovecot port |
| `/admin/settings/iroh_port` | GET/POST | Iroh port |
| `/admin/settings/ss_port` | GET/POST | Shadowsocks port |
| `/admin/settings/smtp_hostname` | GET/POST | SMTP hostname |
| `/admin/settings/turn_realm` | GET/POST | TURN realm |
| `/admin/settings/turn_secret` | GET/POST | TURN secret |
| `/admin/settings/turn_relay_ip` | GET/POST | TURN relay IP |
| `/admin/settings/turn_ttl` | GET/POST | TURN TTL |
| `/admin/settings/iroh_relay_url` | GET/POST | Iroh relay URL |
| `/admin/settings/ss_cipher` | GET/POST | Shadowsocks cipher |
| `/admin/settings/ss_password` | GET/POST | Shadowsocks password |

## Other Tables

### `quota`

Per-user storage quota limits.

| Column | Type | Description |
|--------|------|-------------|
| `username` | string (PK) | User identifier, or `__GLOBAL_DEFAULT__` for the default |
| `max_storage` | int64 | Maximum storage in bytes |
| `created_at` | timestamp | When the quota was set |
| `first_login_at` | timestamp | First login time for the user |

### `dns_overrides`

DNS cache overrides for redirecting outbound mail delivery. See [dns_cache.md](dns_cache.md) for full documentation.

| Column | Type | Description |
|--------|------|-------------|
| `lookup_key` | string (PK) | Domain name or IP address to match (case-insensitive, stored lowercase) |
| `target_host` | string | Destination host/IP to use instead |
| `comment` | string | Optional human-readable note |
| `created_at` | timestamp | Auto-set on creation |
| `updated_at` | timestamp | Auto-set on update |

Managed via: CLI (`maddy dns-cache`), Admin API (`/admin/dns`), or Admin Web UI (DNS tab).

### `blocked_users`

Usernames blocked from re-registration. When an account is deleted via the admin API or CLI, its username is automatically added here.

| Column | Type | Description |
|--------|------|-------------|
| `username` | string (PK) | Blocked email address |
| `reason` | string | Why the user was blocked (e.g. "deleted via admin panel") |
| `blocked_at` | timestamp | When the block was applied |

Blocked usernames cannot register via `/new` or be auto-created via JIT login.

Managed via: Admin API (`/admin/blocklist`) or Admin Web UI (Blocked tab).
