# 04 — Crate-by-Crate Tour

This is your map of the Rust source. For each crate you will find:
- One-sentence purpose
- Key modules / entry points
- Who depends on it
- Rough size / complexity

## Binary & Top-Level Orchestration

### `chatmail` (the binary crate)

**Purpose**: Process entry point, lifecycle, CLI, server supervision, boot, upgrade.

**Key pieces**:
- `main.rs` — tiny dispatcher to `boot::run` or `ctl::dispatch`
- `boot.rs` — `initialize_state`, `run`, hydration
- `supervisor.rs` — `ServerSupervisor` (the heart of the running server)
- `servers.rs` — listener wiring + HTTP router merging (admin + admin-web + www)
- `ctl/` — huge directory with ~25 submodules for every CLI command (`accounts`, `blocklist`, `install`, `reload`, `tasks`, `federation`, etc.)
- `turn_boot.rs`, `iroh_boot.rs`, `ss_boot.rs` — sidecar supervisors
- `logging.rs` — tracing setup + No-Log support

**Depends on**: almost everything.

**Who depends on it**: only integration tests and the binary itself.

## Core Infrastructure Crates

### `chatmail-types`

Shared `ChatmailError` enum + domain helpers (`domain_forms`, `validate_login_domain`, etc.).

Tiny, foundational. Everything else depends on it.

### `chatmail-config`

**Purpose**: All configuration parsing and effective value computation.

- `cli.rs` — clap definition (global flags + many subcommands)
- `parse.rs`, `maddy.rs`, `madmail_parse.rs` — TOML + legacy maddy.conf AST parser
- `install_cli.rs` — `madmail install --simple --ip ...`
- `client_mail.rs` — dclogin links, effective listener addresses per service
- `credential_policy.rs`, `data_size.rs`, `queue.rs`, etc.
- `db_path.rs` — where the app DB and credentials DB live

Heavily used at boot and in ctl commands. The "effective_*" functions are the public API surface for "what port should I listen on?"

### `chatmail-db`

**Purpose**: SQLx pool + all persistence for structured data (not mail bodies).

Modules:
- `pool.rs`, `schema.rs`, `migrations/`
- `settings.rs` + `settings_keys.rs` — the dynamic config heart
- `passwords.rs`, `blocklist.rs`, `registration_tokens.rs`
- `quota_defaults.rs`, `account_info.rs`
- `federation_policy.rs`, `endpoint_cache.rs`
- `message_stats.rs`, `maintenance.rs` (dormant accounts), `message_retention.rs`
- `inbound.rs` — recipient validation helpers

**Critical tables** (from migrations): `settings`, `passwords`, `quotas`, `blocked_users`, `registration_tokens`, `federation_stats`, `dns_overrides`.

Also has Postgres migrations (for future or alternate deploys).

### `chatmail-state`

**Purpose**: In-memory hot path state (the thing that must be fast).

- `AppState` struct + `hydrate`
- `quota.rs` (QuotaCache)
- `policy.rs` (FederationPolicyCache)
- `tracker.rs` (FederationTracker)
- `flusher.rs` — background task that persists stats
- `events.rs` (EventBus for IDLE)
- `silent_dismiss.rs`, `message_size.rs`, `listener_ports.rs`

Almost every delivery path touches this crate under lock.

### `chatmail-storage`

**Purpose**: Maildir abstraction + blob helpers + purge.

- `maildir.rs`, `maildir_message.rs` — folder layout, flag parsing, expunge, move/copy
- `blob.rs` — `write_blob`, `read_blob`, `deliver_local_messages`
- `inbox.rs` — listing
- `purge.rs` — retention / cleanup jobs

Messages live under `<state_dir>/mail/<user>/Maildir/` (with `folders/DeltaChat/Maildir` for the Delta Chat folder).

### `chatmail-auth`

**Purpose**: Authentication + JIT registration.

- `jit.rs` — `authenticate` (the main entry)
- `hash.rs` — password hashing/verification (importable hashes too)
- `normalize.rs` — PRECIS / unicode handling
- `validate.rs` — localpart + password policy

Called from SMTP AUTH, IMAP LOGIN, and `/new`.

### `chatmail-pgp`

**Purpose**: The encryption enforcement gate.

Single important function: `enforce_encryption(raw: &[u8], opts)`.

Accepts:
- `multipart/encrypted` with `application/pgp-encrypted`
- Certain `multipart/mixed` Secure-Join handshakes (`vc-request`, `vg-request`)
- Mailer-daemon `multipart/report` bounces

Rejects everything else. Used on SMTP DATA, IMAP APPEND, and `/mxdeliv`.

## Protocol Server Crates

### `chatmail-smtp`

Custom async SMTP (no external crate for the protocol loop).

- `server.rs` — listener
- `session.rs` — per-connection state machine (EHLO, AUTH, MAIL, RCPT, DATA)
- `protocol.rs` — low-level parsing
- `data_limit.rs`

Two session configs: one for inbound port 25 (no auth required), one for submission (auth required).

### `chatmail-imap`

Custom async IMAP.

- `server.rs`
- `session.rs` — command dispatch
- `connection_stats.rs`

Supports IDLE (push), METADATA (for TURN/Iroh discovery), QUOTA, CONDSTORE-ish bits, MOVE, etc.

### `chatmail-fed`

HTTP server surface for federation.

- `server.rs` — axum listener
- `mxdeliv.rs` — the POST handler (PGP gate + policy + storage)
- `security.rs`

### `chatmail-delivery`

Outbound delivery engine.

- `queue/` — persistent retry queue (store, worker, config)
- `router.rs` — decide HTTPS / HTTP / SMTP path
- `transport.rs` — the actual senders
- `federation_http.rs`

Started once at boot; workers pick up work from the queue.

## Web & Admin Surface Crates

### `chatmail-www`

Public-facing HTTP surface (the one normal users and Delta Chat clients hit).

Routes (see `router.rs`):
- `POST /new` — registration
- `GET/POST /share` — contact sharing
- `/webimap/*` + WebSocket — browser-friendly IMAP subset
- `/websmtp/send`
- `/docs/*` (multi-language)
- `/madmail` — binary download
- `/inv/*` — invites
- Static assets + catch-all

Also contains `www-src/` (source) and `www/` (built) for the classic web UI.

### `chatmail-admin`

JSON-RPC style admin API (`POST /api/admin` or configurable path).

- `router.rs`, `handler.rs`
- `resources/` — one module per domain (accounts, blocklist, federation, quota, settings, tokens, toggles, queue, dns, etc.)

All operations go through a single endpoint + Bearer token. Returns HTTP 200 with `{ "ok": ..., "error": ... }` shape.

### `chatmail-admin-web`

Serves the SvelteKit admin SPA.

- `build.rs` — copies pre-built SPA from `external/madmail-admin-web/build` (or env var) into `embed/` at compile time
- `serve.rs` — axum handler that serves the embedded assets + index.html
- `assets.rs`, `patch.rs`

When the SPA is not embedded, it serves a friendly placeholder.

The SPA itself lives in the `external/` git submodule (Svelte + TS + Tailwind).

## Sidecar / Optional Service Crates

### `chatmail-turn`

In-process TURN/STUN server (based on webrtc-rs / turn-rs work in context/).

- `runner.rs`, `credentials.rs`
- `allocate_client.rs`
- Discovery info for IMAP METADATA

### `chatmail-iroh`

Supervises an `iroh-relay` process (downloaded or built separately).

- `runner.rs`
- `discovery.rs`

### `chatmail-shadowsocks`

Optional port camouflage / proxy.

### `chatmail-tasks`

Background maintenance scheduler.

- `scheduler.rs`, `jobs.rs`
- Retention, dormant account removal, etc.

### `chatmail-metrics`

Prometheus/OpenMetrics exporter.

### `chatmail-acme`

Let's Encrypt HTTP-01 + IP certificate issuance, self-signed fallback.

Used during `madmail install --auto-ip-cert`.

### `chatmail-tls`

Tiny: `load_server_config` (rustls from PEM paths).

## Test / Integration Crate

### `tests` (workspace member `chatmail-integration`)

E2E tests that spin up real servers and talk SMTP/IMAP, do SecureJoin, exercise TURN, run the ctl binary, check OpenMetrics, etc.

Uses support helpers in `tests/support/`.

## Dependency Rules of Thumb

- Low-level crates (`chatmail-types`, `chatmail-config`, `chatmail-db`) have almost no async or protocol logic.
- `chatmail-state` is the only place that should hold cross-cutting in-memory caches.
- Protocol crates (`smtp`, `imap`, `fed`) should not know about the admin API or web UI.
- Anything that touches disk for mail belongs in or under `chatmail-storage`.
- CLI commands in `chatmail/ctl/` are allowed to reach into many crates (they are operator tools).

## How to Decide Where New Code Goes

- New protocol verb or extension → the relevant server crate (`smtp`/`imap`).
- New admin resource → `chatmail-admin/resources/`.
- New background job → `chatmail-tasks`.
- New hot-path cache → `chatmail-state`.
- New storage format or Maildir helper → `chatmail-storage`.
- New config knob → `chatmail-config` + `chatmail-db` settings.

## Next Steps

- See the **runtime wiring** in more detail: [05-boot-sequence-and-state.md](./05-boot-sequence-and-state.md)
- See **exact data flows** for mail and federation in later docs.
- For deep protocol or security rationale, go to the matching `docs/TDD/NN-*.md` file.

This crate map + the architecture diagram will let you answer "where does X live?" in seconds.
