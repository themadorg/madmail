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

### `/admin/accounts` — Account Management
- **GET**: Returns list of all accounts
- **DELETE**: `{"username": "user@domain"}` — Deletes the specified account

Note: Account creation is intentionally excluded. Use the CLI (`maddy creds create`) or the `/new` registration endpoint.

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

## Security Considerations

- The admin token is a shared secret — treat it like a password
- The auto-generated token file (`/var/lib/maddy/admin_token`) is created with `0600` permissions (root-only readable)
- Never expose the admin token in logs or version control
- The API is rate-limited to 10 failed auth attempts per minute per IP
- Accounts cannot be created through the API (passwords never leave the system)
- Use HTTPS in production to protect the token in transit
