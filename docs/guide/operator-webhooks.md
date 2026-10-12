# Operator webhooks

Madmail can POST account registration and storage quota metadata to an operator's
receiver. Configure it in **Admin dashboard → Services → Operator webhooks** or
through the authenticated admin RPC API. It is disabled by default. Enable the
embedded dashboard with `madmail admin-web enable`, apply route changes with
`madmail reload`, and obtain API credentials with `madmail admin-token --no-qr`.

## Configure and test

1. Set an HTTPS receiver URL and optionally a signing secret.
2. Choose registration and quota events; enable the service and save.
3. Select **Send test webhook**. This uses saved settings, including timeout and
   retries, and reports the actual receiver result.

Blank secret input preserves the current secret. **Remove the current secret**
clears it explicitly. API responses report `secret_configured`, never the secret.
Configuration is operator-only and is not published on `/new` or public pages.

| Setting | Default | Accepted values |
| --- | --- | --- |
| `enabled` | `false` | Boolean master switch |
| `url` | Empty | HTTPS; HTTP only for `localhost`, `127.0.0.1`, or `[::1]` testing |
| `secret` | Empty | Optional HMAC key; omitted/null preserves, empty string clears |
| `event_user_registered` | `true` | Boolean |
| `event_quota_exceeded` | `true` | Boolean |
| `timeout_seconds` | `5` | Integer, 1–30 |
| `retry_attempts` | `2` | Integer, 0–5 additional attempts |

URLs are limited to 2,048 bytes and secrets to 4,096 bytes; control characters
are rejected. URLs cannot contain embedded credentials or fragments. TLS certificate validation
is enabled; redirects and environment proxies are disabled. Settings persist in
SQLite or PostgreSQL, take effect immediately, and survive configuration reloads.
They are stored as the private `__OPERATOR_WEBHOOKS__` JSON setting, managed through
the dedicated API rather than generic settings or file configuration.

## Admin RPC API

Send POST requests to the configured admin API path (default `/api/admin`). The
Bearer credential belongs **inside the JSON envelope's `headers` object**.
The `resource` is `/admin/services/webhooks`; it is not a separate HTTP route.
There is no dedicated `madmail webhooks` CLI command.

| Envelope method | Body | Result |
| --- | --- | --- |
| `GET` | `{}` | Redacted settings and runtime counters |
| `PUT` | Partial settings object | Saves valid fields and returns redacted settings |
| `POST` | `{"action":"test"}` | Sends `webhook.test` using saved enabled settings |

For example, save this as a private JSON request file, replacing the placeholders:

```json
{
  "method": "PUT",
  "resource": "/admin/services/webhooks",
  "headers": { "Authorization": "Bearer <admin-token>" },
  "body": {
    "enabled": true,
    "url": "https://hooks.example.org/madmail",
    "secret": "<signing-key>",
    "event_user_registered": true,
    "event_quota_exceeded": true,
    "timeout_seconds": 5,
    "retry_attempts": 2
  }
}
```

```sh
chmod 600 webhook-request.json
curl --fail-with-body --silent --show-error \
  -H 'Content-Type: application/json' --data-binary @webhook-request.json \
  https://mail.example.org/api/admin
```

Remove the request file when finished because it contains credentials. GET and
POST use the same envelope with the method/body shown in the table. HTTP responses
carry an RPC envelope: check its `status` and `error`, not only curl's exit status.
Successful operations have RPC status 200; malformed settings return 400,
unauthorized calls 401, and disabled/failed test delivery 502.

## Payloads and receiver verification

```json
{"event":"user.registered","timestamp":1770000000,"username":"user@example.org","source":"jit","registration_token_used":false}
```

`source` is `jit`, `web`, or `admin`. Only successful new-account provisioning
emits an event: repeated logins and imports of existing users do not. Concurrent
JIT logins and admin imports share a per-user lock to avoid duplicate creation
events. Invite usage is a boolean; the invitation itself is never sent.
Host-side CLI account creation does not emit this server-runtime event.

```json
{"event":"user.quota_exceeded","timestamp":1770000000,"username":"user@example.org","used_bytes":100,"max_bytes":100,"incoming_bytes":50,"path":"imap.append"}
```

Quota rejections use `imap.append`, `smtp.inbound`, `smtp.submission`, `websmtp`,
`federation.inbound`, `mxdeliv`, or `admin.notice` paths. WebIMAP APPEND shares
`imap.append`. Rejections for one username across paths coalesce for one hour.
Mail rejection behavior remains the same whether delivery of the alert succeeds
or fails. Payloads contain no passwords, message content, device IDs or invite IDs.

```json
{"event":"webhook.test","timestamp":1770000000,"message":"madmail operator webhook test"}
```

With a configured secret, verify `X-Madmail-Signature: sha256=<hex>` using
HMAC-SHA256 over the **raw request bytes** and a constant-time comparison. Do not
parse and reserialize JSON before verification. Without a secret the header is
omitted. Timestamps are Unix seconds; receivers must enforce their own freshness
and replay policy and make automation idempotent because retries repeat payloads.

## Delivery limits and troubleshooting

Any 2xx response succeeds. Transport errors, 429 and 5xx responses retry with
1, 2, 4, 8 and 16 second backoffs, bounded by `retry_attempts`. Other statuses,
including redirects, fail without retry. Each request has the configured timeout.
Mail and login paths enqueue without awaiting HTTP delivery.

The queue holds 256 events and the hourly quota deduplication map holds up to
4,096 usernames. Capacity exhaustion drops new events. This is best-effort
in-memory delivery: queued events and deduplication state reset on restart.
Changing configuration cancels pending events/retries; an in-flight request can
finish with its previous settings. Unchanged configuration reloads retain them.

GET reports `successful_deliveries` (completed events/tests),
`consecutive_failures` (failed attempts since the last success), and
`dropped_events` (queue/dedup capacity drops). Counters survive soft reloads but
reset on restart. If a test fails, check the saved URL, receiver response, TLS and
secret. Warning logs identify attempts/status codes without recording the URL,
secret, username or payload; the existing No-Log configuration still applies.

For implementation and verification commands, see
[the design](../TDD/25-operator-webhooks.md) and
[operator instructions](../project/user-guide/07-admin-and-cli.md#operator-webhooks).
