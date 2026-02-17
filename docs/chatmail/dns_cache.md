# DNS Cache & Override System

Madmail includes a database-backed DNS override cache that intercepts outbound mail delivery resolution. When the server needs to deliver a message to an external domain, it first checks the local override database before performing standard OS DNS lookups.

## How It Works

The resolution flow for outbound delivery is:

1. **Check local database** — If a `dns_overrides` entry exists for the domain or IP, use its `target_host` value.
2. **Use original hostname** — If no override is found, the original hostname is used as-is for DNS resolution and TLS verification.

This applies at two points in the delivery pipeline:

- **MX lookup** — When resolving which mail server to connect to for a recipient domain.
- **Host resolution** — When resolving the MX hostname itself to an IP address for the TCP connection.

**Important:** When no override exists, the original hostname is preserved for proper TLS certificate verification and MTA-STS compatibility. Only explicit overrides bypass these security checks.

## Configuration

The DNS cache requires a `storage` directive in your `target.remote` block that points to the storage module. The `dns_overrides` table is stored in the **same database** as the rest of the application (quotas, passwords, settings, etc.):

```
target.remote outbound_delivery {
    storage local_mailboxes
    # ... other directives
}
```

This connects to the same `storage.imapsql` database used by the rest of the application. No separate database file is needed.

## Use Cases

### Route Mail to a Specific Server

Override where mail for `nine.testrun.org` is delivered:

```
lookup_key: nine.testrun.org
target_host: 10.0.0.5
comment: Route to internal staging server
```

Now any email sent to `user@nine.testrun.org` will be delivered to `10.0.0.5` instead of the domain's real MX record.

### Override IP-Literal Destinations

When someone sends to an IP-literal address like `user@[1.1.1.1]`, you can redirect it:

```
lookup_key: 1.1.1.1
target_host: 2.2.2.2
comment: Redirect IP literal delivery
```

### Testing and Migration

During server migration, temporarily redirect all outbound mail for a domain to the new server without updating public DNS:

```
lookup_key: example.com
target_host: new-mx.example.com
comment: Migration - redirect to new MX
```

## Database Schema

The feature uses a GORM-managed table `dns_overrides` in the main application database:

| Column | Type | Description |
|--------|------|-------------|
| `lookup_key` | string (PK) | Domain name or IP address to match (case-insensitive, stored lowercase) |
| `target_host` | string | Destination host/IP to use instead |
| `comment` | string | Optional human-readable note |
| `created_at` | timestamp | Auto-set on creation |
| `updated_at` | timestamp | Auto-set on update |

The table is automatically created/migrated when the DNS cache is initialized.

## Integration

The DNS cache is wired into the `target.remote` module. It obtains the shared GORM database from the storage module via the `module.GORMProvider` interface:

```go
// In target.remote Init:
// cfg.String("storage", ..., &storageName)
// storageInst := module.GetInstance(storageName)
// gormProvider := storageInst.(module.GORMProvider)
// cache := dns_cache.New(gormProvider.GetGORMDB(), logger)
```

### Key Behaviours

- **Case-insensitive** — `NINE.TESTRUN.ORG` matches `nine.testrun.org`.
- **Trailing dots stripped** — `example.com.` matches `example.com`.
- **Bracket-aware** — `[1.1.1.1]` and `[ipv6:::1]` are normalized before lookup.
- **TLS preserved** — When an override redirects to an IP, TLS verification is relaxed (InsecureSkipVerify) since IP certificates are uncommon. When no override exists, the original hostname is used for proper TLS cert verification.
- **No fallback to OS resolver** — `Resolve()` only returns results for explicit database overrides. If no override exists, it returns empty so the standard Go dialer handles DNS resolution with proper hostname-based TLS.

## CLI Commands

The `maddy dns-cache` commands read the `storage.imapsql` block from your config to connect to the same database as the running server:

```bash
# List all overrides
maddy dns-cache list

# Set an override
maddy dns-cache set nine.testrun.org 10.0.0.5 "Route to staging"

# View an override
maddy dns-cache get nine.testrun.org

# Remove an override
maddy dns-cache remove nine.testrun.org
```

## Programmatic API

The `dns_cache.Cache` type provides these methods:

```go
// Resolution (returns empty string if no override exists)
cache.Resolve(ctx, "example.com")         // → "1.2.3.4" or ""
cache.ResolveMX(ctx, "example.com")       // → []*net.MX, cacheHit bool

// CRUD
cache.Set("example.com", "1.2.3.4", "note")
cache.Get("example.com")                  // → *DNSOverride
cache.Delete("example.com")
cache.List()                              // → []DNSOverride
```

## Notes

- Overrides bypass all MX authentication policies (MTA-STS, DANE, DNSSEC) since the admin is explicitly choosing the destination.
- This feature is designed for server operators, not end users. The override database should only be accessible to admins.
- Changes take effect immediately — no restart is required after adding or modifying an override.
