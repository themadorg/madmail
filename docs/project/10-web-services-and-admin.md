# 10 — Web Services and Admin API (chatmail-www + chatmail-admin)

This layer is what normal users, Delta Chat desktop, and operators interact with over HTTP.

## Public Surface: `chatmail-www`

Crate: `crates/chatmail-www`

This is the "face" of a chatmail server for browsers and lightweight clients.

### Major Routes (see `router.rs`)

- `POST /new` — account creation (with optional registration token)
- `GET /` + static assets — the classic landing page / info
- `/docs/*` — multi-language documentation (en, fa, ru, es, ...)
- `/webimap/*` + `GET /webimap/ws` — REST + WebSocket subset of IMAP for browser / desktop clients
- `/websmtp/send` — lightweight message submission (used by some web clients)
- `/share` and `/inv/*` — contact sharing and invite links
- `/madmail` — download the current server binary (useful for bootstrap)
- `/app` — Delta Chat app download links

It also serves the admin docs and database schema docs under `/docs/...`.

### Static Assets

The crate contains both:
- `www-src/` — source HTML/JS/CSS (the "classic" web UI)
- `www/` — the built/copied version that gets embedded

`build.rs` copies things at compile time when needed.

### WebIMAP / WebSocket Path

For Delta Chat desktop and web clients that don't want to speak raw IMAP over TLS, there is a REST + WebSocket translation layer (`webimap.rs`, `webimap_ws.rs`).

This lets a browser tab maintain a "live" mailbox view without a full IMAP stack.

## Admin API: `chatmail-admin`

Crate: `crates/chatmail-admin`

Single JSON-RPC-style endpoint:

```http
POST /api/admin
Content-Type: application/json

{"method":"GET","resource":"/admin/accounts","headers":{"Authorization":"Bearer <token>"},"body":{}}
```

Use the path configured by `admin_path`. Authentication belongs inside the JSON
`headers` object; the resource and method select the operation.

Every admin operation (list accounts, ban, set quota, toggle registration, view queue, federation stats, etc.) goes through this one endpoint.

### Resources

See `resources/` directory — one file per domain:
- `accounts.rs`
- `blocklist.rs`
- `federation.rs`
- `quota.rs`
- `settings.rs`
- `tokens.rs` (registration tokens)
- `webhooks.rs` — `/admin/services/webhooks` settings and actual test delivery
- `toggles.rs`
- `queue.rs`, `message_size.rs`, `port.rs`, etc.

The handler dispatches on the resource path and method string.

### Auth

- Bearer token (constant-time compare).
- Token can come from the `admin_token` file or be overridden in static config (or set to the literal string "disabled").
- Rate limiting is applied.

RPC responses use HTTP 200 and contain `status`, `resource`, `body`, `error`,
and `version`. Check the envelope status/error for operation failures.

### Admin Web SPA

The operator dashboard is the `external/madmail-admin-web` SvelteKit app, embedded via `chatmail-admin-web`.

When you run `make build-with-admin-web`:
1. The SPA is built to `external/madmail-admin-web/build`
2. `chatmail-admin-web/build.rs` copies it into `crates/chatmail-admin-web/embed/`
3. At runtime the admin-web crate serves the SPA assets under the configured path (default `/admin` or via `admin_web_path`).

The SPA talks exclusively to the JSON-RPC admin endpoint (it does not have its own backend).

If the SPA is not embedded, the server returns a friendly placeholder page telling the operator how to build it.

### Why Embed the SPA?

Self-contained deployment. One `scp` of the `madmail` binary + one restart gives you the full operator UI. No separate nginx + static hosting step.

### Operator webhooks

Services → Operator webhooks manages the private receiver URL, optional signing
secret, event switches, timeout/retries and test delivery. The authenticated
`/admin/services/webhooks` resource supports GET, PUT and POST; it never returns
the signing secret. Settings persist and activate immediately.

`AppState.webhooks` delivers registration and quota metadata through a bounded
in-memory worker. Mail/auth paths only enqueue; receiver failure cannot change
the protocol result. Concurrent JIT and admin provisioning share the per-user
creation lock, and quota rejections coalesce hourly per username. The dispatcher
lives in `chatmail-state`; no new external process is required.

See [the operator guide](../guide/operator-webhooks.md) for exact envelopes,
payloads, signing and delivery limits, and [TDD 25](../TDD/25-operator-webhooks.md)
for the design. `make build-with-admin-web` embeds the Services form.

## Relationship to the Old Madmail Admin

The original Go Madmail had an `internal/adminweb` that was also a built Svelte app embedded via Go `//go:embed`.

The Rust version deliberately reuses the same (or very similar) Svelte source from the `external/` submodule so that operators get a consistent UI.

## Public vs Admin Separation

- `chatmail-www` router is the "catch-all" and serves user-facing things.
- Admin routes are merged **in front** of the www catch-all (see `servers::merge_http_routers`).
- This means `/api/admin` and `/admin` (SPA) take precedence over any user content that might collide.

## Testing the Web Layers

- `cargo test -p chatmail-www`
- `cargo test -p chatmail-state webhooks`
- `cargo test -p chatmail-integration --test operator_webhooks_e2e`
- `make test-docker` — workspace, landing and shipping-image checks
- E2E tests that hit `/new`, do WebIMAP operations, etc.
- The admin API is exercised heavily by the ctl commands and the Svelte SPA itself during manual testing.

## Common Operator Workflows

1. Enable registration: Admin UI → Settings → toggle, or `madmail registration open`
2. Create a registration token for a specific user: Admin UI or `madmail registration-tokens create`
3. Ban a user: Admin UI blocklist or CLI
4. Inspect federation health: Admin UI federation tab (uses the tracker stats)
5. Change message size limit or default quota: settings resource

All of the above ultimately write to the `settings` table or other DB tables and are picked up on next hydration or via cache invalidation.

## Next

The web and admin layers sit on top of everything. The final major piece of the core server is **storage + persistence + the in-memory caches**.

→ [11-proxy-services-turn-iroh-ss.md](./11-proxy-services-turn-iroh-ss.md)
