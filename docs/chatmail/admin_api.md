# Admin API

Madmail includes a built-in Admin API accessible through a single RPC-style endpoint.
All requests are `POST /api/admin` with a JSON body.

## Design Principles

1. **Single endpoint** — One path, one POST handler, easier to protect behind firewalls
2. **Bearer token auth** — Token stored in config, passed in inner request headers
3. **No sensitive data in responses** — Passwords and keys are never exposed
4. **Rate limiting** — 10 auth attempts per minute per IP
5. **JSON-RPC style** — Method, resource, headers, and body in a single request envelope

## Authentication

The Admin API is **enabled by default** with an auto-generated token. On first startup,
Madmail generates a random 43-character token and stores it at `/var/lib/maddy/admin_token`.

### Retrieving the Token

```bash
maddy admin-token
```

### Configuration Options

The token can be customized or disabled in `maddy.conf`:

```
chatmail tls://0.0.0.0:443 {
    # Auto-generated (default) — no config needed

    # Set a custom token:
    # admin_token your-custom-token

    # Disable the admin API entirely:
    # admin_token disabled
}
```

## Request Format

```json
{
    "method": "GET|POST|PUT|DELETE",
    "resource": "/admin/status",
    "headers": {
        "Authorization": "Bearer your-secret-token-here"
    },
    "body": {}
}
```

## Response Format

```json
{
    "status": 200,
    "resource": "/admin/status",
    "body": { ... },
    "error": null
}
```

## Available Resources

### `/admin/status` — Server Status
- **GET**: Returns uptime, user count, and email server tracking data

Example:
```bash
curl -X POST https://your-server/api/admin \
  -H 'Content-Type: application/json' \
  -d '{
    "method": "GET",
    "resource": "/admin/status",
    "headers": {"Authorization": "Bearer YOUR_TOKEN"}
  }'
```

Response:
```json
{
    "status": 200,
    "resource": "/admin/status",
    "body": {
        "users": {"registered": 42},
        "uptime": {"boot_time": "2026-02-17T10:00:00Z", "duration": "2d 5h 30m"},
        "email_servers": {
            "connection_ips": 15,
            "domain_servers": 8,
            "ip_servers": 3
        }
    }
}
```

### `/admin/storage` — Storage Information
- **GET**: Returns disk usage, state directory size, and database info

### `/admin/registration` — User Registration Toggle
- **GET**: Returns whether registration is open or closed
- **POST**: `{"action": "open"}` or `{"action": "close"}`

### `/admin/registration/jit` — JIT Registration Toggle
- **GET**: Returns whether JIT registration is enabled
- **POST**: `{"action": "enable"}` or `{"action": "disable"}`

### `/admin/services/turn` — TURN Server Toggle
- **GET**: Returns whether TURN is enabled
- **POST**: `{"action": "enable"}` or `{"action": "disable"}`

### `/admin/services/iroh` — Iroh Relay Toggle
- **GET**: Returns whether Iroh relay is enabled
- **POST**: `{"action": "enable"}` or `{"action": "disable"}`

### `/admin/services/shadowsocks` — Shadowsocks Toggle
- **GET**: Returns whether Shadowsocks is enabled
- **POST**: `{"action": "enable"}` or `{"action": "disable"}`

### `/admin/services/log` — Logging Toggle
- **GET**: Returns whether logging is disabled (No Log Policy)
- **POST**: `{"action": "enable"}` or `{"action": "disable"}`

### `/admin/settings` — Bulk Settings (Read-Only)
- **GET**: Returns all settings (toggles + ports + config) in one response

### Port Settings

Each port setting endpoint supports:
- **GET**: Returns the current value and whether it is explicitly set
- **POST**: `{"action": "set", "value": "2525"}` or `{"action": "reset"}`

| Endpoint | Description |
|----------|-------------|
| `/admin/settings/smtp_port` | SMTP server port |
| `/admin/settings/submission_port` | Submission server port |
| `/admin/settings/imap_port` | IMAP server port |
| `/admin/settings/turn_port` | TURN relay port |
| `/admin/settings/dovecot_port` | Dovecot SASL port |
| `/admin/settings/iroh_port` | Iroh relay port |
| `/admin/settings/ss_port` | Shadowsocks proxy port |

### Configuration Settings

Each configuration endpoint supports the same GET/POST (set/reset) pattern:

| Endpoint | Description |
|----------|-------------|
| `/admin/settings/smtp_hostname` | SMTP server hostname |
| `/admin/settings/turn_realm` | TURN server realm |
| `/admin/settings/turn_secret` | TURN shared secret |
| `/admin/settings/turn_relay_ip` | TURN relay IP address |
| `/admin/settings/turn_ttl` | TURN credential TTL (seconds) |
| `/admin/settings/iroh_relay_url` | Iroh relay URL |
| `/admin/settings/ss_cipher` | Shadowsocks cipher algorithm |
| `/admin/settings/ss_password` | Shadowsocks password |

Example — set a port:
```json
{"method": "POST", "resource": "/admin/settings/smtp_port",
 "headers": {"Authorization": "Bearer TOKEN"},
 "body": {"action": "set", "value": "2525"}}
```

Example — read a setting:
```json
{"method": "GET", "resource": "/admin/settings/turn_secret",
 "headers": {"Authorization": "Bearer TOKEN"}}
```

Response:
```json
{"status": 200, "body": {"key": "__TURN_SECRET__", "value": "mysecret", "is_set": true}}
```

Example — reset to default:
```json
{"method": "POST", "resource": "/admin/settings/smtp_port",
 "headers": {"Authorization": "Bearer TOKEN"},
 "body": {"action": "reset"}}
```

### `/admin/accounts` — Account Management
- **GET**: Returns list of all accounts with storage usage
- **DELETE**: `{"username": "user@domain"}` — Fully deletes the account and blocks the username

Deletion removes:
1. Authentication credentials
2. IMAP storage and all messages
3. Quota records
4. Adds the username to the blocklist (prevents re-registration)

Note: Account creation is intentionally excluded. Use the CLI (`maddy creds create`) or the `/new` registration endpoint.

### `/admin/blocklist` — Blocked Users Management
- **GET**: Returns list of all blocked usernames
- **POST**: `{"username": "user@domain", "reason": "optional reason"}` — Block a username
- **DELETE**: `{"username": "user@domain"}` — Unblock a username

Blocked usernames cannot register via `/new` or be auto-created via JIT login.
Deleted accounts are automatically added to the blocklist.

Example — list blocked users:
```json
{"method": "GET", "resource": "/admin/blocklist",
 "headers": {"Authorization": "Bearer TOKEN"}}
```

Response:
```json
{"status": 200, "body": {
    "total": 2,
    "blocked": [
        {"username": "spammer@domain", "reason": "deleted via admin panel", "blocked_at": "2026-02-18T15:30:00Z"},
        {"username": "abuser@domain", "reason": "manually blocked", "blocked_at": "2026-02-18T14:00:00Z"}
    ]
}}
```

Example — manually block a user:
```json
{"method": "POST", "resource": "/admin/blocklist",
 "headers": {"Authorization": "Bearer TOKEN"},
 "body": {"username": "spammer@domain", "reason": "abuse"}}
```

Example — unblock a user:
```json
{"method": "DELETE", "resource": "/admin/blocklist",
 "headers": {"Authorization": "Bearer TOKEN"},
 "body": {"username": "spammer@domain"}}
```

### `/admin/quota` — Quota Management
- **GET**: Returns storage stats (or per-user quota if `{"username": "..."}` provided)
- **PUT**: Set quota — `{"username": "user@domain", "max_bytes": 1073741824}` (omit username for default)
- **DELETE**: Reset user quota to default — `{"username": "user@domain"}`

### `/admin/queue` — Queue Management
- **POST**: Message purge operations:
  - `{"action": "purge_user", "username": "user@domain"}`
  - `{"action": "purge_all"}`
  - `{"action": "purge_read"}`

### `/admin/shares` — Contact Shares
Only available when contact sharing is enabled.
- **GET**: List all contact shares
- **POST**: Create a share — `{"slug": "myslug", "url": "openpgp4fpr:...", "name": "Display Name"}`
- **PUT**: Update a share — `{"slug": "myslug", "url": "...", "name": "..."}`
- **DELETE**: Delete a share — `{"slug": "myslug"}`

### `/admin/dns` — DNS Cache Overrides
Only available when the storage module provides a GORM DB.
- **GET**: List all DNS overrides
- **POST**: Create/update an override — `{"lookup_key": "example.com", "target_host": "1.2.3.4", "comment": "..."}`
- **DELETE**: Delete an override — `{"lookup_key": "example.com"}`

See [dns_cache.md](dns_cache.md) for detailed documentation on the DNS override system.

## Web Admin Panel

All Admin API resources are also accessible through a built-in web interface at `/admin/`. The panel uses the same authentication token and provides:

| Tab | Features |
|-----|----------|
| **Overview** | Server stats (users, uptime, disk, storage), connection counts (IMAP, TURN, Shadowsocks), disk usage bar, queue purge buttons |
| **Services** | Toggle switches for Registration, JIT, TURN, Iroh, Shadowsocks, and Logging |
| **Ports** | View and modify all service port numbers and configuration values |
| **Accounts** | List all accounts with storage usage, delete accounts (with confirmation modal) |
| **Blocked** | View blocked usernames, unblock users (with confirmation modal) |
| **DNS** | View, add, search, and delete DNS overrides (with confirmation modal) |

The panel supports both **light and dark mode** via a toggle in the header, and is available in English, Farsi, Spanish, and Russian.

## Security Considerations

- The admin token is a shared secret — treat it like a password
- The auto-generated token file (`/var/lib/maddy/admin_token`) is created with `0600` permissions (root-only readable)
- Never expose the admin token in logs or version control
- The API is rate-limited to 10 failed auth attempts per minute per IP
- Accounts cannot be created through the API (passwords never leave the system)
- Use HTTPS in production to protect the token in transit
