# Operator webhooks

Issue [#74](https://github.com/themadorg/madmail/issues/74). Owned by
`chatmail-state::webhooks`, `chatmail-admin::resources::webhooks`, registration
hooks in auth/www/admin, and quota hooks in IMAP/SMTP/delivery/federation.
The admin UI lives in `external/madmail-admin-web` under Services.

## Contract

Disabled by default. One operator-selected URL receives JSON metadata:

```json
{"event":"user.registered","timestamp":1770000000,"username":"user@example.org","source":"jit","registration_token_used":false}
```

`source` is `jit`, `web`, or `admin`. Only successfully provisioned new accounts
emit registration events; existing-account logins, hash upgrades and imports
that update existing accounts do not. Web registration includes only a boolean
indicating invite usage, never the invite or password. Concurrent JIT logins and admin provisioning share
the existing per-user creation lock and emit one event.

```json
{"event":"user.quota_exceeded","timestamp":1770000000,"username":"user@example.org","used_bytes":100,"max_bytes":100,"incoming_bytes":50,"path":"imap.append"}
```

A rejected quota check emits metadata without changing the existing rejection or
PGP policy. Paths: `imap.append` (also WebIMAP's shared APPEND handler),
`smtp.inbound`, `smtp.submission`, `websmtp`, `mxdeliv`, `federation.inbound`,
and `admin.notice`. Quota events coalesce per user for one hour across paths.
An event disabled by policy or dropped before enqueueing does not reserve a
dedup entry. The cache holds at most 4096 users; at capacity, new alerts are
counted as dropped rather than evicting an unexpired entry and duplicating it.
Expired entries permit another alert. Dedup is runtime state, reset on restart.

No message bodies, password hashes, passwords, device tokens or registration
secrets are serialized. These are explicitly enabled operator disclosures of
account metadata, consistent with No-Log's separation of mail content from
operator state. Endpoint URLs, secrets, usernames and HTTP response bodies are
absent from delivery warning logs.

## Admin resource and storage

Use the existing authenticated RPC envelope at the configured admin API path:

```json
{"method":"GET","resource":"/admin/services/webhooks","headers":{"Authorization":"Bearer <admin-token>"},"body":{}}
```

| Method | Behavior |
| --- | --- |
| GET | Redacted configuration and runtime counters |
| PUT | Partial update, validate, persist, then activate without restart |
| POST | `{"action":"test"}` sends a test using saved, enabled configuration |

PUT fields: `enabled`, `url`, `secret`, `event_user_registered`,
`event_quota_exceeded`, `timeout_seconds` (integer 1–30; default 5),
`retry_attempts` (integer 0–5; default 2). Both event switches default true but
are inactive until the master enable switch is true. Omitted fields preserve
current values. Omitted/null secret preserves it; `"secret":""` clears it.
Unknown fields and invalid types are rejected. GET/PUT/POST responses never
include the secret, only `secret_configured`.

Store one JSON value in the existing DB settings resource under
`__OPERATOR_WEBHOOKS__`, supporting SQLite and PostgreSQL and the legacy Madmail
KV layout. Configuration updates are serialized; validation happens before DB
writes and activation after persistence. Boot and soft reload hydrate it. An
unchanged reload preserves pending jobs; changed settings invalidate queued
jobs to avoid delivering old metadata to a new endpoint. A request already in
flight can complete with its original settings. The secret is stored in the
operator-protected DB, so backups require the same protection as credentials.
It is never supplied to public templates, registration responses, generic
settings snapshots or the browser's persistent storage.

GET exposes `successful_deliveries` (completed event/test deliveries),
`consecutive_failures` (failed attempts since the last success), and
`dropped_events` (queue/dedup capacity drops). Counters survive soft reload but
reset on process restart. Test payload:

```json
{"event":"webhook.test","timestamp":1770000000,"message":"madmail operator webhook test"}
```

Success returns RPC status 200; disabled/failed test delivery returns 502 with
sanitized diagnostics. An explicit operator test awaits delivery, while auth,
SMTP and IMAP never await webhook network I/O.

## Delivery and security

An in-process bounded channel (256 jobs) feeds one worker. Hot paths use
`try_send`; full queues drop metadata with a warning and counter. No sidecar or
mandatory external service. Jobs are memory-only: crashes/restarts can lose
pending events. Persistent retry storage is deliberately optional in v1; these
webhooks are best-effort notifications, not an audit or transaction log.

Use HTTPS with normal certificate validation. HTTP is allowed only for explicit
loopback receivers (`localhost`, `127.0.0.1`, `[::1]`) in local testing. Reject
URL credentials, fragments and control characters. Reject redirects (including
HTTPS-to-HTTP), bypass inherited HTTP proxy settings, bound connect/request time,
and do not read response bodies. Only authenticated operators can choose the
destination; they may deliberately use internal HTTPS services. This is not a
public URL-fetching API. Loopback HTTP should never traverse an untrusted proxy.

`Content-Type: application/json`. With a nonempty secret, send
`X-Madmail-Signature: sha256=<lowercase hex>` using HMAC-SHA256 over the exact
request bytes. Receivers must verify before parsing, compare signatures in
constant time, and use the signed timestamp to reject stale/replayed payloads.
Retries reuse the same body/signature: receivers must tolerate duplicate
processing if a response was lost after accepting an event.

Retry transport failures, HTTP 429 and 5xx with bounded exponential delays
(1, 2, 4, … seconds). Other non-2xx statuses fail immediately. A 2xx resets the
failure streak and increments successes. A configuration change cancels pending
retries before another attempt. No logs include arbitrary reqwest error text
because it can contain credential-bearing URLs. Dependencies reuse workspace
`reqwest`, `hmac`, `sha2`, `hex`, Serde and Tokio versions already in Cargo.lock.

## Verification

`cargo test -p chatmail-state webhooks` covers a known HMAC vector, payload/header
bytes, redaction, URL/policy validation, omitted/cleared secrets, event gates,
hourly dedup/expiry/capacity, bounded queue, generation cancellation, retries,
redirect rejection, request timeout, counters and DB/reload hydration.

`cargo test -p chatmail-integration --test operator_webhooks_e2e` exercises actual
admin RPC authorization, all three registration sources, concurrent admin imports/JIT and
repeat-login suppression, invite-token privacy,
IMAP APPEND and SMTP quota rejection/dedup, per-event disabling, saved-config test,
soft hydration and public-page isolation with a local mock receiver. Existing
protocol tests cover PGP gates and unchanged delivery behavior.

The admin form uses the shared API client, follows server connection changes,
never preloads or persists the secret, supports explicit clearing, and tests only
saved configuration. UI tests lock the distinction between blank and cleared
secrets and prevent response counters from being written back as configuration.
