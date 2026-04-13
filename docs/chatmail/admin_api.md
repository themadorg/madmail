# Admin API

Madmail includes a built-in Admin API accessible through a single RPC-style endpoint.
All requests are `POST /api/admin` with a JSON body.

## Design Principles

1. **Single endpoint** — One path, one POST handler, easier to protect behind firewalls
2. **Bearer token auth** — Token stored in config, passed in inner request headers
3. **No sensitive data in responses** — Passwords and keys are never exposed
4. **Rate limiting** — 10 auth attempts per minute per IP
5. **JSON-RPC style** — Method, resource, headers, and body in a single request envelope
6. **Constant HTTP 200** — The real status is inside the JSON body (prevents observers from distinguishing auth failures, errors, and successes by HTTP status code)

## Authentication

The Admin API is **enabled by default** with an auto-generated token. On first startup
(or the first time `maddy admin-token` is run), Madmail generates a random 43-character
token and stores it at `/var/lib/<binary>/admin_token`.

### Retrieving the Token

```bash
maddy admin-token          # Pretty-printed with API URL
maddy admin-token --raw    # Raw token only (for scripting)
TOKEN=$(maddy admin-token --raw)
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
    "method": "GET|POST|PUT|DELETE|PATCH",
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
    "body": { "..." },
    "error": null
}
```

> **Note:** The outer HTTP response is always `200 OK`. The actual status code is in the `status` field of the JSON body. This prevents network observers from distinguishing auth failures from successful requests.

## Available Resources

### `/admin/status` — Server Status
- **GET**: Returns uptime, user count, live connection counts, message counters, and email server tracking data

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
        "uptime": {"boot_time": "2026-02-17T10:00:00Z", "duration": "2d 5h 30m 0s"},
        "email_servers": {
            "connection_ips": 15,
            "domain_servers": 8,
            "ip_servers": 3
        },
        "imap": {"connections": 23, "unique_ips": 18},
        "turn": {"relays": 2},
        "shadowsocks": {"connections": 5, "unique_ips": 4},
        "sent_messages": 1024,
        "outbound_messages": 512,
        "received_messages": 2048
    }
}
```

### `/admin/storage` — Storage Information
- **GET**: Returns disk usage, state directory size, and database file info

Response:
```json
{
    "status": 200,
    "body": {
        "disk": {"total_bytes": 50000000000, "used_bytes": 12000000000, "available_bytes": 38000000000, "percent_used": 24.0},
        "state_dir": {"path": "/var/lib/maddy", "size_bytes": 256000000},
        "database": {"driver": "sqlite3", "size_bytes": 4200000}
    }
}
```

### `/admin/restart` — Service Restart
- **POST**: Schedules a `systemctl restart` of the service (responds before restart completes)

Response:
```json
{"status": 200, "body": {"status": "restarting", "message": "Service restart initiated. Please wait a few seconds."}}
```

### `/admin/reload` — Configuration Reload
- **POST**: Regenerates the configuration file from DB-stored overrides and triggers a graceful restart via SIGUSR2

Response:
```json
{"status": 200, "body": {"status": "reloading", "message": "Configuration regenerated. Service is restarting."}}
```

---

## Service Toggles

Each toggle endpoint supports:
- **GET**: Returns `{"enabled": true/false}`
- **POST**: `{"action": "enable"}` or `{"action": "disable"}`

| Endpoint | Description |
|----------|-------------|
| `/admin/registration` | Open/close user registration via `/new` |
| `/admin/registration/jit` | Enable/disable JIT (Just-In-Time) account creation on IMAP/SMTP login |
| `/admin/services/turn` | Enable/disable TURN relay (video calls) |
| `/admin/services/iroh` | Enable/disable Iroh relay (Webxdc realtime) |
| `/admin/services/shadowsocks` | Enable/disable Shadowsocks proxy |
| `/admin/services/ss_ws` | Enable/disable Shadowsocks WebSocket transport |
| `/admin/services/ss_grpc` | Enable/disable Shadowsocks gRPC transport |
| `/admin/services/http_proxy` | Enable/disable HTTP proxy |
| `/admin/services/log` | Enable/disable logging (No-Log Policy) |
| `/admin/services/admin_web` | Enable/disable the admin web dashboard (instant, no restart) |

Example — disable Shadowsocks:
```json
{"method": "POST", "resource": "/admin/services/shadowsocks",
 "headers": {"Authorization": "Bearer TOKEN"},
 "body": {"action": "disable"}}
```

---

## Settings

### `/admin/settings` — Bulk Settings (Read-Only)
- **GET**: Returns all toggles, ports, configuration values, and local-only flags in one response

### Generic Setting Endpoints

Each individual setting endpoint supports:
- **GET**: Returns `{"key": "__KEY__", "value": "...", "is_set": true/false}`
- **POST**: `{"action": "set", "value": "..."}` or `{"action": "reset"}`

#### Port Settings

| Endpoint | DB Key | Description |
|----------|--------|-------------|
| `/admin/settings/smtp_port` | `__SMTP_PORT__` | SMTP server port |
| `/admin/settings/submission_port` | `__SUBMISSION_PORT__` | Submission server port |
| `/admin/settings/imap_port` | `__IMAP_PORT__` | IMAP server port |
| `/admin/settings/turn_port` | `__TURN_PORT__` | TURN relay port |
| `/admin/settings/sasl_port` | `__SASL_PORT__` | Dovecot SASL port |
| `/admin/settings/iroh_port` | `__IROH_PORT__` | Iroh relay port |
| `/admin/settings/ss_port` | `__SS_PORT__` | Shadowsocks proxy port |
| `/admin/settings/ss_ws_port` | `__SS_WS_PORT__` | Shadowsocks WebSocket port |
| `/admin/settings/ss_grpc_port` | `__SS_GRPC_PORT__` | Shadowsocks gRPC port |
| `/admin/settings/http_port` | `__HTTP_PORT__` | Chatmail HTTP port |
| `/admin/settings/https_port` | `__HTTPS_PORT__` | Chatmail HTTPS port |
| `/admin/settings/http_proxy_port` | `__HTTP_PROXY_PORT__` | HTTP proxy port |

#### Local-Only Access Control

Bind individual services to `127.0.0.1` only (effective after restart):

| Endpoint | DB Key | Description |
|----------|--------|-------------|
| `/admin/settings/smtp_local_only` | `__SMTP_LOCAL_ONLY__` | SMTP listen on localhost only |
| `/admin/settings/submission_local_only` | `__SUBMISSION_LOCAL_ONLY__` | Submission listen on localhost only |
| `/admin/settings/imap_local_only` | `__IMAP_LOCAL_ONLY__` | IMAP listen on localhost only |
| `/admin/settings/turn_local_only` | `__TURN_LOCAL_ONLY__` | TURN listen on localhost only |
| `/admin/settings/iroh_local_only` | `__IROH_LOCAL_ONLY__` | Iroh listen on localhost only |
| `/admin/settings/http_local_only` | `__HTTP_LOCAL_ONLY__` | HTTP listen on localhost only |
| `/admin/settings/https_local_only` | `__HTTPS_LOCAL_ONLY__` | HTTPS listen on localhost only |

#### Configuration Settings

| Endpoint | DB Key | Description |
|----------|--------|-------------|
| `/admin/settings/smtp_hostname` | `__SMTP_HOSTNAME__` | SMTP server hostname |
| `/admin/settings/turn_realm` | `__TURN_REALM__` | TURN server realm |
| `/admin/settings/turn_secret` | `__TURN_SECRET__` | TURN shared secret |
| `/admin/settings/turn_relay_ip` | `__TURN_RELAY_IP__` | TURN relay IP address |
| `/admin/settings/turn_ttl` | `__TURN_TTL__` | TURN credential TTL (seconds) |
| `/admin/settings/iroh_relay_url` | `__IROH_RELAY_URL__` | Iroh relay URL |
| `/admin/settings/ss_cipher` | `__SS_CIPHER__` | Shadowsocks cipher algorithm |
| `/admin/settings/ss_password` | `__SS_PASSWORD__` | Shadowsocks password |
| `/admin/settings/http_proxy_path` | `__HTTP_PROXY_PATH__` | HTTP proxy URL path |
| `/admin/settings/http_proxy_username` | `__HTTP_PROXY_USERNAME__` | HTTP proxy username |
| `/admin/settings/http_proxy_password` | `__HTTP_PROXY_PASSWORD__` | HTTP proxy password |
| `/admin/settings/admin_path` | `__ADMIN_PATH__` | Admin API URL path |
| `/admin/settings/admin_web_path` | `__ADMIN_WEB_PATH__` | Admin web dashboard URL path |
| `/admin/settings/language` | `__LANGUAGE__` | UI language (`en`, `fa`, `ru`, `es`) |

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

---

## Account & User Management

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

---

## Messaging

### `/admin/notice` — Admin Notice Emails
Send an unencrypted admin notice email to one or all users.

- **GET**: Returns total user count and mail domain
- **POST**: Send a notice

Example — broadcast to all users:
```json
{"method": "POST", "resource": "/admin/notice",
 "headers": {"Authorization": "Bearer TOKEN"},
 "body": {"subject": "Server Maintenance", "body": "The server will restart at 03:00 UTC."}}
```

Example — send to a single user:
```json
{"method": "POST", "resource": "/admin/notice",
 "headers": {"Authorization": "Bearer TOKEN"},
 "body": {"subject": "Account Warning", "body": "Your account has been flagged.", "recipient": "user@domain"}}
```

Response:
```json
{"status": 200, "body": {"sent": 42, "failed": 0}}
```

---

## External Services

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

### `/admin/exchangers` — Exchanger Management
Manage pull-based email relay exchangers.
- **GET**: List all configured exchangers
- **POST**: Add a new exchanger — `{"name": "relay1", "url": "https://relay.example.com/mxdeliv", "poll_interval": 60}`
- **PUT**: Update an exchanger — `{"name": "relay1", "enabled": false, "poll_interval": 120}`
- **DELETE**: Remove an exchanger — `{"name": "relay1"}`

Example response for GET:
```json
{
    "status": 200,
    "body": {
        "exchangers": [
            {
                "name": "relay1",
                "url": "https://relay.example.com/mxdeliv",
                "enabled": true,
                "poll_interval": 60,
                "last_poll_at": "2026-04-02T11:51:00Z"
            }
        ],
        "total": 1
    }
}
```
---

## Federation Management

Federation controls determine which remote domains can send to or receive from this server.

### `/admin/settings/federation` — Policy Toggle
- **GET**: Returns `{"enabled": true/false, "policy": "ACCEPT|REJECT"}`
- **POST**: `{"enabled": true}` and/or `{"policy": "REJECT"}`

Policy modes:
| Policy | Rules Act As | Default Behavior |
|--------|-------------|------------------|
| `ACCEPT` | **Blocklist** | Allow all except listed domains |
| `REJECT` | **Allowlist** | Deny all except listed domains |

Example — enable federation with REJECT policy:
```json
{"method": "POST", "resource": "/admin/settings/federation",
 "headers": {"Authorization": "Bearer TOKEN"},
 "body": {"enabled": true, "policy": "REJECT"}}
```

### `/admin/federation/rules` — Domain Exception CRUD
- **GET**: List all rules
- **POST**: `{"domain": "bad.com"}` — Add a domain to the exception list
- **DELETE**: `{"domain": "bad.com"}` — Remove a domain from the exception list

Example — add a domain rule:
```json
{"method": "POST", "resource": "/admin/federation/rules",
 "headers": {"Authorization": "Bearer TOKEN"},
 "body": {"domain": "spammer.com"}}
```

GET response:
```json
{
    "status": 200,
    "body": {
        "rules": [
            {"domain": "spammer.com", "created_at": 1712000000}
        ],
        "total": 1
    }
}
```

### `/admin/federation/servers` — Traffic Diagnostics
- **GET**: Returns per-domain delivery statistics from RAM (no DB hit)

Response:
```json
{
    "status": 200,
    "body": {
        "servers": [
            {
                "domain": "remote.example.com",
                "queued_messages": 2,
                "failed_http": 0,
                "failed_https": 1,
                "failed_smtp": 0,
                "success_http": 0,
                "success_https": 15,
                "success_smtp": 3,
                "successful_deliveries": 18,
                "mean_latency_ms": 245.5,
                "last_active": 1712345678
            }
        ],
        "total": 1
    }
}
```

---

## Web Admin Panel

All Admin API resources are also accessible through a built-in web interface at `/admin/`. The panel uses the same authentication token and provides:

| Tab | Features |
|-----|----------|
| **Overview** | Server stats (users, uptime, disk, storage), live connection counts (IMAP, TURN, Shadowsocks), message counters, queue purge buttons |
| **Services** | Toggle switches for Registration, JIT, TURN, Iroh, Shadowsocks (TCP/WS/gRPC), HTTP Proxy, Logging, and Admin Web |
| **Ports** | View and modify all service port numbers, local-only access flags, and configuration values |
| **Accounts** | List all accounts with storage usage, delete accounts (with confirmation modal) |
| **Blocked** | View blocked usernames, unblock users (with confirmation modal) |
| **DNS** | View, add, search, and delete DNS overrides (with confirmation modal) |
| **Federation** | Toggle federation enforcement (ACCEPT/REJECT), manage domain rules (add/delete), view per-domain traffic diagnostics with transport breakdown (HTTPS/HTTP/SMTP) |
| **Exchangers** | Manage pull-based email relay exchangers |
| **Notice** | Send admin notice emails to one or all users |

The panel supports **light and dark mode** via a toggle in the header, and is available in English, Farsi, Spanish, and Russian.

## Security Considerations

- The admin token is a shared secret — treat it like a password
- The auto-generated token file (`/var/lib/<binary>/admin_token`) is created with `0600` permissions (root-only readable)
- Never expose the admin token in logs or version control
- The API is rate-limited to 10 failed auth attempts per minute per IP (stale entries auto-cleaned every 5 minutes)
- Accounts cannot be created through the API (passwords never leave the system)
- Use HTTPS in production to protect the token in transit
- Request bodies are limited to 1 MB (enforced before authentication to prevent memory exhaustion DoS)
- Authentication uses constant-time comparison to prevent timing attacks
- All responses return HTTP 200 to prevent status-code-based information leakage
