# Architecture Overview

## Rust workspace (`crates/`)

The repository is a Cargo workspace (`Cargo.toml` at repo root). The shipped binary is **`chatmail`** (`crates/chatmail`, deployed as `/usr/local/bin/madmail` on test hosts). Library crates are split by protocol and hot-path concern so admin, CLI, and integration tests can depend on logic without pulling the full server.

### Dependency layers

```
                    ┌─────────────┐
                    │  chatmail   │  binary: boot, supervisor, ctl, *_boot
                    └──────┬──────┘
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
    chatmail-fed    chatmail-imap   chatmail-smtp
    chatmail-www    chatmail-admin  chatmail-admin-web
    chatmail-turn   chatmail-iroh   chatmail-shadowsocks
    chatmail-push   chatmail-metrics
           │               │               │
           └───────────────┼───────────────┘
                           ▼
              chatmail-delivery ──► chatmail-pgp
              chatmail-storage ◄── chatmail-auth
                           │
                           ▼
              chatmail-state (AppState, caches, flusher, push)
                           │
                           ▼
              chatmail-db ◄── chatmail-config
                           │
                           ▼
                    chatmail-types
```

Sidecars (used at boot, not in the diagram above): `chatmail-tls`, `chatmail-acme`, `chatmail-tasks`.

Integration tests live in workspace member `tests/` (`chatmail-integration` package).

### Crate reference

| Crate | Role | Key modules / entry points |
|-------|------|------------------------------|
| **`chatmail`** | Process entry: `main` → `boot::run` or `ctl::dispatch` | `admin`, `boot`, `supervisor`, `servers`, `ctl/*`, `logging`, `push_boot`, `turn_boot`, `iroh_boot`, `ss_boot`, `upgrade`; optional `profiling` (`pprof` feature) |
| **`chatmail-types`** | Shared errors and domain helpers | `error`, `domains` |
| **`chatmail-config`** | `maddy.conf` AST, `AppConfig`, CLI (`clap`) | `maddy`, `madmail_parse`, `parse`, `cli`, `install_cli`, `config_autocert`, `config_www`, `credential_policy`, `queue`, `data_size`, `client_mail`, `autoconfig`, `paths`, `db_path` |
| **`chatmail-db`** | SQLx pool (SQLite + Postgres), migrations, settings, accounts | `pool`, `settings`, `settings_keys`, `passwords`, `blocklist`, `endpoint_cache`, `federation_policy`, `message_stats`, `message_retention`, `maintenance`, `mail_ports`, `modseq`, `inbound`, `sharing`, `registration_tokens`, `account_info`, `schema`, `models`, `quota_defaults` |
| **`chatmail-state`** | In-memory hot path hydrated at boot | `AppState` (`mailbox_store`, `push`, `webhooks`, `jit_flights`), `auth`, `quota`, `policy`, `tracker`, `flusher`, `events`, `message_size`, `silent_dismiss`, `listener_ports`, `reload` (`ReloadScope::{Full,HttpRoutes}`) |
| **`chatmail-storage`** | Maildir + CAS blobs on disk | `maildir`, `blob`, `cas`, `external_store`, `storage_policy`, `uidlist`, `maildir_cache`, `fsync_batch`, `delivery_batch`, `maildir_message`, `purge`, `inbox` |
| **`chatmail-auth`** | Login, JIT, password hashing | `jit`, `hash`, `validate`, `normalize` |
| **`chatmail-pgp`** | PGP-only policy gate (MIME/header checks via `mail-parser`) | `enforce_encryption`, `EnforceOptions` |
| **`chatmail-smtp`** | Custom async SMTP listener + sessions | `server`, `session`, `protocol`, `data_limit` |
| **`chatmail-imap`** | Async IMAP listener + IDLE | `server`, `session`, `connection_stats` |
| **`chatmail-fed`** | HTTP listener: `/mxdeliv` + merged routers | `mxdeliv`, `security`, `server::run_http_listener` |
| **`chatmail-delivery`** | Outbound queue (HTTP then SMTP) | `queue`, `router`, `transport`; private `federation_http` (shared `reqwest` client) |
| **`chatmail-push`** | XDELTAPUSH device tokens + `notifications.delta.chat` notifier | `enabled`, `notifier`, `store`, `mode`, `stats` — [23-push-notifications.md](23-push-notifications.md) |
| **`chatmail-webhooks`** | Operator HTTPS webhooks (registration, quota cap) | `WebhookDispatcher`, settings keys — [24-operator-webhooks.md](24-operator-webhooks.md) |
| **`chatmail-www`** | Public site, `/new`, WebIMAP/WebSMTP | `router`, `webimap`, `webimap_ws`, `handlers`, `gate`, `export`, `assets`, `context_cache`, `template`, `response` |
| **`chatmail-admin`** | Admin JSON-RPC (`POST /api/admin`) | `auth`, `cors`, `handler`, `router`, `resources/*` |
| **`chatmail-admin-web`** | Embedded operator SPA | `serve::admin_web_router` |
| **`chatmail-tls`** | Load PEM → `rustls::ServerConfig` | `load_server_config` |
| **`chatmail-acme`** | Let's Encrypt via **instant-acme** + self-signed | `obtain`, `obtain_ip`, `self_signed`, `http01`, `status`, `acme_common`; helpers `is_valid_dns_domain`, `is_valid_ip_for_acme`, `read_certificate_info` |
| **`chatmail-turn`** | In-process TURN/STUN (**webrtc-rs `turn` 0.11**) | `runner`, `credentials`, `parse`, `turn_client`, `allocate_client` (`turn_allocate`, `TurnDiscovery`) |
| **`chatmail-iroh`** | Supervise embedded `iroh-relay` v0.35 | `runner`, `discovery` (`IrohDiscovery`) |
| **`chatmail-shadowsocks`** | Optional camouflage proxy | `server`, `runtime`, `cipher`, `urls`, `xray`, `allowed_ports` (`resolve_runtime`, `ShadowsocksUrls`) |
| **`chatmail-tasks`** | Scheduled maintenance + autocert renewal | `scheduler`, `jobs`, `config`, `cert_renew` (`CertificateRenewer` trait) |
| **`chatmail-metrics`** | Prometheus OpenMetrics exporter | `metrics`, `server` (SMTP counters, queue gauge) |

### Runtime wiring

`chatmail::supervisor::ServerSupervisor` starts listeners and background work:

1. **Boot** (`boot.rs`) — state dir, `chatmail-db` migrate, admin token, `AppState::hydrate`, message-stats + federation flusher.
2. **HTTP** (`chatmail-fed`) — binds plain/TLS; base router is `/mxdeliv`; `chatmail::servers::build_http_extra` merges admin API, admin-web SPA, and `chatmail-www` routes.
3. **SMTP / submission** (`chatmail-smtp`) — port 25 + configured submission listeners.
4. **IMAP** (`chatmail-imap`) — plain/TLS; METADATA for TURN/Iroh discovery and `XDELTAPUSH` device tokens.
5. **Outbound** (`chatmail-delivery::start_outbound_queue`) — persistent queue + transport.
6. **Push** (`push_boot`) — XDELTAPUSH notifier when `__PUSH_MODE__` is not `off`.
7. **Proxies** — `turn_boot`, `iroh_boot`, `ss_boot` when enabled in settings/CLI.
8. **Maintenance** (`chatmail-tasks::spawn_maintenance_scheduler`) — retention, dormant accounts, auto-purge seen, daily autocert renewal when `tls_mode = autocert`.
9. **Metrics** (`chatmail-metrics`) — optional OpenMetrics listener.

Reload: admin `POST /admin/reload` or signal path. `ReloadScope::HttpRoutes` rebinds HTTP only; `ReloadScope::Full` stops SMTP/IMAP/HTTP, re-hydrates `AppState`, and recreates listeners with updated ports/TLS from DB + config (`supervisor.rs`).

### TDD section → crate map

| TDD doc | Primary crates |
|---------|----------------|
| [02-smtp-server.md](02-smtp-server.md) | `chatmail-smtp`, `chatmail-pgp`, `chatmail-auth`, `chatmail-delivery` |
| [03-imap-server.md](03-imap-server.md) | `chatmail-imap`, `chatmail-storage`, `chatmail-state` |
| [04-storage-layer.md](04-storage-layer.md) | `chatmail-storage`, `chatmail-state`, `chatmail-db` |
| [05-authentication.md](05-authentication.md) | `chatmail-auth`, `chatmail-db` |
| [07-federation.md](07-federation.md) | `chatmail-fed`, `chatmail-delivery`, `chatmail-state`, `chatmail-db` |
| [09-admin-api.md](09-admin-api.md) | `chatmail-admin`, `chatmail-admin-web` |
| [10-webimap.md](10-webimap.md) | `chatmail-www` |
| [11-proxy-services.md](11-proxy-services.md) | `chatmail-turn`, `chatmail-iroh`, `chatmail-shadowsocks` |
| [13-configuration.md](13-configuration.md) | `chatmail-config`, `chatmail-db` |
| [14-cli-tools.md](14-cli-tools.md) | `chatmail`, `chatmail-config` — operator usage: [`../guide/cli/`](../guide/cli/README.md) |
| [16-testing.md](16-testing.md) | `tests/` + per-crate `tests/` (e.g. `chatmail-turn`) |
| [17-data-models.md](17-data-models.md) | `chatmail-db/migrations/` |
| [19-certificates.md](19-certificates.md) | `chatmail-acme`, `chatmail-tls` |
| [21-scheduled-maintenance.md](21-scheduled-maintenance.md) | `chatmail-tasks`, `chatmail-storage`, `chatmail-db` |
| [23-push-notifications.md](23-push-notifications.md) | `chatmail-push`, `chatmail-imap`, `chatmail-admin` |

Normative protocol specs used across these crates are archived under [`RFC/`](RFC/README.md) (plain-text `rfc*.txt` + TURN REST draft). Each TDD section links the relevant local files in its **Related RFCs** table.

## High-Level Components

```
┌─────────────────────────────────────────────────────────────────┐
│                        madmail-v2                               │
├─────────────────────────────────────────────────────────────────┤
│  HTTP Server (Axum)                                            │
│   ├── /new                  (Registration)                     │
│   ├── /mxdeliv              (Federation receive)               │
│   ├── /webimap/*            (WebIMAP REST + WebSocket)         │
│   └── /api/admin            (Admin RPC endpoint)               │
├─────────────────────────────────────────────────────────────────┤
│  SMTP Server (custom async SMTP)                               │
│   ├── Submission (465/587)  — requires encryption              │
│   └── Incoming (25)         — federation + external            │
├─────────────────────────────────────────────────────────────────┤
│  IMAP Server (custom async IMAP)                               │
│   ├── IDLE push                                                  │
│   ├── METADATA (TURN/Iroh discovery, XDELTAPUSH devicetoken)   │
│   └── QUOTA extension                                            │
├─────────────────────────────────────────────────────────────────┤
│  Core Services (High-Throughput)                               │
│   ├── Auth (in-memory users + JIT)                               │
│   ├── Filesystem Mail Storage (Maildir + symlinks)               │
│   ├── In-Memory Hot Data (Users, Quotas, Rules, Metrics)         │
│   ├── FederationTracker (pure in-memory + periodic flush)        │
│   └── Background Persistence Manager                             │
├─────────────────────────────────────────────────────────────────┤
│  Integrated proxy services (same deployment unit)              │
│   ├── TURN server (`chatmail-turn` / webrtc-rs in-process)     │
│   └── Iroh relay (`chatmail-iroh` supervises iroh-relay v0.35) │
└─────────────────────────────────────────────────────────────────┘
```

## Technology Stack (implemented)

| Layer              | Technology                          | Rationale |
|--------------------|-------------------------------------|---------|
| Async Runtime      | Tokio + tracing                     | Widely used in Rust servers |
| HTTP + WebSocket   | Axum + tower                        | WebSocket support; familiar Rust HTTP stack |
| TLS                | rustls + tokio-rustls               | Memory safe, modern |
| Database           | SQLx (SQLite default; Postgres supported) | Async friendly; Madmail schema import |
| SMTP Server        | Custom `chatmail-smtp` (`protocol`, `data_limit`) | Chatmail PGP + federation + JIT; Stalwart studied for session shape |
| IMAP Server        | Custom `chatmail-imap` on `chatmail-storage` | IDLE, METADATA, XDELTAPUSH |
| PGP policy         | `mail-parser` MIME/header checks (`chatmail-pgp`) | Madmail parity without OpenPGP packet parser |
| ACME               | instant-acme + custom HTTP-01 solver (`chatmail-acme`) | DNS + IP short-lived certs |
| TURN               | webrtc-rs `turn` 0.11 in-process (`chatmail-turn`) | TURN REST HMAC credentials |
| Config             | `maddy.conf` + DB settings + soft reload | Dynamic settings |
| CLI                | `clap` (`madmail` binary, `chatmail` crate) | Interactive install |
| Admin Web          | Embedded Madmail Svelte SPA (`chatmail-admin-web`) | Same-origin with `/api/admin` |

## Core Data Flow

### 1. User Registration (JIT or /new)
- `/new` or first IMAP/SMTP login → `auth.pass_table` creates entry if JIT enabled
- IMAP mailbox lazily created on first delivery or access

### 2. Message Submission (User → Server)
```
Delta Chat → Submission (465/587) 
  → PGP check (only encrypted or SecureJoin allowed)
  → local delivery OR remote target.remote
```

### 3. Inbound Federation
```
Remote Server → POST /mxdeliv (or SMTP)
  → Federation policy check (ACCEPT/REJECT + rules)
  → PGP enforcement
  → Storage + quota update
```

### 4. Outbound Federation
```
target.remote
  → Check endpoint cache / endpoint_rewrite
  → Try HTTPS POST /mxdeliv
  → Fallback HTTP
  → Fallback SMTP (MX lookup)
  → Update FederationTracker (latency, failures, queue)
```

### 5. Admin Operations
All go through single `POST /api/admin` JSON-RPC endpoint with Bearer token.

## Key In-Memory Hot Paths (Performance Critical)

- **FederationPolicyCache** (`Arc<FederationPolicyCache>`) — global ACCEPT/REJECT + per-domain rules
- **FederationSilentDismissCache** — outbound domains accepted but not delivered
- **QuotaCache** (`Arc<QuotaCache>`)
- **AuthCache** — credentials + blocklist + JIT flag (not full user objects)
- **FederationTracker** (per-domain stats, updated on every delivery attempt)
- **Endpoint overrides** — DB-backed (`chatmail-db::endpoint_cache`); read on outbound routing
- **Settings** — DB source of truth; hot paths read via `AppState` fields or per-request DB fetch

All modifications are **write-through** (DB + RAM) under lock.

## Concurrency Model

- Main HTTP/SMTP/IMAP listeners: Tokio tasks
- Delivery pipeline: spawned tasks per message
- Background workers:
  - Message counter flusher (30s)
  - FederationTracker flusher (30s)
  - Config reloader (on signal or API trigger)
- IMAP IDLE: per-connection task with notify channels

## Security Boundaries

- All external input (SMTP, IMAP, HTTP) goes through strict validation
- Admin API protected by constant-time Bearer token + rate limiting
- No sensitive data (passwords, private keys) ever returned in responses
- HTTP 200 always for Admin API (status inside JSON)

## Single Binary Layout

```
madmail                     # clap name in production; dev crate: chatmail
├── madmail run            # default: full server (boot + supervisor)
├── madmail install        # chatmail-config::install_cli
├── madmail <subcommand>   # accounts, federation, certificate, tasks, …
├── systemd unit template
└── embedded assets (chatmail-www www-src, chatmail-admin-web SPA)
```

Operator reference (per-command): [`../guide/cli/README.md`](../guide/cli/README.md). Design parity matrix: [14-cli-tools.md](14-cli-tools.md).

This matches Madmail's philosophy of simple deployment. The product is still referred to as **madmail-v2** in design docs; the workspace crate is **`chatmail`**, the shipped binary name is **`madmail`**.

## Implementation references

Index: [`CONTEXT.md`](CONTEXT.md).

| Component (this doc) | madmail | cmrelay | cmdeploy | stalwart |
|----------------------|---------|---------|----------|----------|
| HTTP (`/new`, `/mxdeliv`, admin) | [`internal/endpoint/chatmail/`](../../context/madmail/internal/endpoint/chatmail/) | [`filtermail/src/mxdeliv.rs`](../../context/cmrelay/src/filtermail/src/mxdeliv.rs) | nginx deployer | [`crates/http/`](../../context/stalwart/crates/http/) |
| SMTP | [`internal/endpoint/smtp/`](../../context/madmail/internal/endpoint/smtp/) | [`smtp_server.rs`](../../context/cmrelay/src/filtermail/src/smtp_server.rs) | [`postfix/`](../../context/cmdeploy/src/cmdeploy/postfix/) | [`crates/smtp/`](../../context/stalwart/crates/smtp/) |
| IMAP | [`internal/endpoint/imap/`](../../context/madmail/internal/endpoint/imap/) | Dovecot via deploy | [`dovecot.conf.j2`](../../context/cmdeploy/src/cmdeploy/dovecot/dovecot.conf.j2) | [`crates/imap/`](../../context/stalwart/crates/imap/) |
| Storage + hot RAM | [`internal/storage/imapsql/`](../../context/madmail/internal/storage/imapsql/), [`internal/quota/`](../../context/madmail/internal/quota/), [`internal/federationtracker/`](../../context/madmail/internal/federationtracker/) | [`chatmaild/`](../../context/cmrelay/src/filtermail/python/chatmaild/) | Dovecot maildir | [`crates/store/`](../../context/stalwart/crates/store/), [`crates/email/`](../../context/stalwart/crates/email/) |
| Outbound delivery | [`internal/target/remote/`](../../context/madmail/internal/target/remote/) | [`outbound.rs`](../../context/cmrelay/src/filtermail/src/outbound.rs) | Postfix relay | [`crates/smtp/src/outbound/`](../../context/stalwart/crates/smtp/src/outbound/) |
| Install / single binary | [`internal/cli/ctl/install.go`](../../context/madmail/internal/cli/ctl/install.go) | [`manager/internal/install/`](../../context/cmrelay/src/manager/internal/install/) | [`cmdeploy.py`](../../context/cmdeploy/src/cmdeploy/cmdeploy.py) | [`install.sh`](../../context/stalwart/install.sh) |

## Related RFCs

Protocols and transports at the system boundary. Index: [`RFC/README.md`](RFC/README.md).

| RFC | Role in architecture | Local |
|-----|----------------------|-------|
| [5321](https://datatracker.ietf.org/doc/html/rfc5321) | SMTP (inbound + outbound fallback) | [rfc5321.txt](RFC/rfc5321.txt) |
| [5322](https://datatracker.ietf.org/doc/html/rfc5322) | Message format (federation, storage) | [rfc5322.txt](RFC/rfc5322.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP4rev1 | [rfc3501.txt](RFC/rfc3501.txt) |
| [9110](https://datatracker.ietf.org/doc/html/rfc9110) | HTTP (`/mxdeliv`, `/new`, Admin API, WebIMAP) | [rfc9110.txt](RFC/rfc9110.txt) |
| [8446](https://datatracker.ietf.org/doc/html/rfc8446) | TLS 1.3 (SMTP, IMAP, HTTPS) | [rfc8446.txt](RFC/rfc8446.txt) |