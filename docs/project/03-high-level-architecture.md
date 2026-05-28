# 03 — High-Level Architecture

This is the "30,000 foot view" that lets you reason about the system without drowning in details.

## The One Binary That Does Everything

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           chatmail (the binary)                              │
├─────────────────────────────────────────────────────────────────────────────┤
│  Listeners (started by ServerSupervisor)                                    │
│  ├─ SMTP (25) + Submission (465/587)   ← chatmail-smtp                      │
│  ├─ IMAP (143/993)                     ← chatmail-imap + IDLE + METADATA    │
│  ├─ HTTP (80/443 + extra)              ← chatmail-fed (/mxdeliv)            │
│  │                                        + chatmail-www (public site)       │
│  │                                        + chatmail-admin (JSON-RPC)        │
│  │                                        + embedded admin-web SPA           │
│  ├─ TURN/STUN (for calls)              ← chatmail-turn (in-process)         │
│  ├─ Iroh relay                         ← chatmail-iroh (supervises binary)  │
│  └─ Shadowsocks (optional camouflage)  ← chatmail-shadowsocks               │
├─────────────────────────────────────────────────────────────────────────────┤
│  Background & Hot Path                                                   │
│  ├─ AppState (in-memory caches: quota, policy, federation tracker, events) │
│  ├─ Periodic flusher (stats → DB)                                          │
│  ├─ Outbound delivery queue (chatmail-delivery)                            │
│  ├─ Maintenance scheduler (retention, dormant accounts)                    │
│  └─ Metrics exporter (optional)                                            │
├─────────────────────────────────────────────────────────────────────────────┤
│  Storage (durable)                                                         │
│  ├─ SQLite (chatmail.db) — settings, accounts, quotas, stats, policy...    │
│  └─ Maildir on disk (data/mail/<user>/Maildir + DeltaChat/ folders)        │
└─────────────────────────────────────────────────────────────────────────────┘
```

Everything above runs in **one Tokio process**. This is a deliberate design choice for simple deployment (one binary, one systemd unit, one set of ports).

## Layered Crate Architecture (Dependency Flow)

```
chatmail (binary + ctl + boot + supervisor)
    │
    ├─► chatmail-www, chatmail-admin, chatmail-admin-web, chatmail-fed
    │   (HTTP surfaces)
    │
    ├─► chatmail-smtp, chatmail-imap
    │   (protocol servers)
    │
    ├─► chatmail-delivery (outbound queue + router + transports)
    │
    ├─► chatmail-turn, chatmail-iroh, chatmail-shadowsocks (sidecar supervisors)
    │
    └─► chatmail-config
            │
            └─► chatmail-db (SQLx + migrations + DAOs)
                    │
                    └─► chatmail-state (AppState + caches + flusher)
                            │
                            ├─► chatmail-storage (Maildir)
                            ├─► chatmail-auth
                            └─► chatmail-pgp
```

Lower layers have **no knowledge** of HTTP or SMTP framing. Higher layers orchestrate.

Side crates (`chatmail-types`, `chatmail-tls`, `chatmail-acme`, `chatmail-tasks`, `chatmail-metrics`) are small and cross-cutting.

## Key Runtime Components

### 1. Boot Sequence (`boot.rs` + `supervisor.rs`)

1. Parse CLI + load static config (TOML or maddy.conf syntax).
2. Create state dir, initialize/open SQLite, run migrations.
3. Resolve admin token (from file or config).
4. Create `AppState` (in-memory hot data).
5. `AppState::hydrate()` — load quotas, policies, message stats from DB + disk.
6. Start background flusher task.
7. Start optional sidecar servers (TURN, Iroh, SS).
8. Start protocol listeners (SMTP, IMAP, HTTP).
9. Start maintenance scheduler.
10. Start OpenMetrics listener if configured.
11. Enter reload loop (on SIGHUP or `POST /admin/reload`).

`ServerSupervisor` owns all listeners and the reload channel. On reload it tears down listeners, re-hydrates, and rebinds.

### 2. AppState — The Hot Path

Defined in `chatmail-state`.

Holds:
- `QuotaCache` (per-user used/max, checked on every delivery)
- `FederationPolicyCache` (ACCEPT/REJECT rules)
- `FederationTracker` (per-domain latency/failure stats, flushed periodically)
- `MessageSizeLimit`
- `EventBus` (for IMAP IDLE notifications)
- `MailboxStore` (Maildir abstraction)
- `ListenerPortsStore`

These are `Arc` + `RwLock` or similar. Almost every hot path (SMTP DATA, /mxdeliv, APPEND, quota checks) goes through them. Modifications are usually write-through (update RAM + schedule DB write).

### 3. Storage Split

- **Durable structured data** → SQLite (`chatmail-db`)
- **Message bodies + flags** → Maildir on disk (`chatmail-storage`)
- **Hot working set** → RAM inside `AppState` (with periodic flush)

This gives both speed (RAM checks) and durability (disk + DB).

### 4. Federation Model

**Inbound (remote → us)**:
- Preferred: `POST /mxdeliv` (HTTP) with raw message in body + `X-Mail-From` header
- Fallback: SMTP to port 25
- Always goes through `chatmail-pgp::enforce_encryption`
- Policy check, quota check, local delivery via storage

**Outbound (us → remote)**:
- `chatmail-delivery` queue worker
- First tries HTTPS POST /mxdeliv (using cached or discovered endpoint)
- Falls back to HTTP, then traditional SMTP (MX lookup)
- Tracks success/failure/latency in `FederationTracker`

### 5. Authentication & Identity

- Passwords stored as hashes in DB (via `chatmail-auth`).
- **JIT registration**: if no account exists and JIT is enabled (via settings or `registration_open`), first successful password attempt creates the account + Maildir + quota row.
- `/new` endpoint also creates accounts (optionally with registration tokens).
- Blocklist checked on every auth and delivery attempt.

### 6. Proxy / Sidecar Services

- **TURN** (`chatmail-turn`): WebRTC relay for Delta Chat calls. Credentials and server info served to clients via IMAP `GETMETADATA` (special server entries).
- **Iroh** (`chatmail-iroh`): Supervises an embedded `iroh-relay` binary for p2p/WebXDC.
- **Shadowsocks**: Optional port-forwarding camouflage so the mail ports look like a proxy.

These are started at boot when configured and can be toggled at runtime via admin settings.

## Concurrency & Reliability Model

- Tokio multi-thread runtime.
- Each connection (SMTP, IMAP, HTTP) is a spawned task.
- Delivery and outbound workers are spawned tasks.
- Background tasks: flusher (30s?), maintenance scheduler, reload watcher.
- Graceful shutdown on Ctrl-C: flusher is told to finish, then exit.
- Listeners use `CancellationToken` for coordinated shutdown on reload.

## Security Boundaries (High Level)

- All external input validated (PGP gate is the big one).
- Admin API protected by constant-time bearer token comparison + rate limiting.
- No sensitive material (password hashes, private keys) ever returned in API responses.
- TLS everywhere possible (rustls).
- ACME (Let's Encrypt) or self-signed or manual certs supported.

## Single-Binary Deployment Benefits

- One `systemd` unit.
- One set of firewall rules.
- One thing to `scp` and restart on deploy.
- Admin web and docs travel with the binary (when embedded).

Trade-off: if one component crashes hard, the whole service restarts. In practice this has been acceptable.

## Where to Go Next

You now have the mental model.

- For the **detailed crate responsibilities** → [04-crate-by-crate-tour.md](./04-crate-by-crate-tour.md)
- For the **exact boot code path** → [05-boot-sequence-and-state.md](./05-boot-sequence-and-state.md)
- For **data flow deep dives** (registration, send mail, federation) → the later numbered docs + `docs/TDD/`

The TDD/01-architecture.md is also excellent and complements this view.
