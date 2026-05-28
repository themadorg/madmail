# 02 — Getting the Code: Repository Layout

This section teaches you how to navigate the repository like a native.

## Top-Level View (most important things)

```
madmailv2/                          # repo root (this directory)
├── Cargo.toml                      # Rust workspace definition (21+ members)
├── Makefile                        # The most important file for humans
├── LICENCE
├── universal_test_tool.md
├── scripts/                        # Build, deploy, E2E
│   ├── build-release-static.sh
│   ├── core-e2e*.sh
│   └── ...
├── crates/                         # ← THE RUST SOURCE (your main focus)
│   ├── chatmail/                   # Binary crate + orchestration (main, boot, supervisor, ctl)
│   ├── chatmail-smtp/              # SMTP server + session
│   ├── chatmail-imap/              # IMAP server + IDLE + METADATA
│   ├── chatmail-fed/               # HTTP federation listener (/mxdeliv)
│   ├── chatmail-delivery/          # Outbound queue + transports
│   ├── chatmail-www/               # Public web (registration, docs, WebIMAP)
│   ├── chatmail-admin/             # Admin JSON-RPC API
│   ├── chatmail-admin-web/         # Embedded Svelte admin SPA + assets
│   ├── chatmail-auth/              # JIT, hashing, validation
│   ├── chatmail-pgp/               # Encryption enforcement gate
│   ├── chatmail-db/                # SQLx + migrations + DAOs
│   ├── chatmail-state/             # In-memory hot caches + flusher
│   ├── chatmail-storage/           # Maildir on disk
│   ├── chatmail-config/            # maddy.conf / toml parsing + CLI
│   ├── chatmail-turn/              # TURN/STUN server wrapper
│   ├── chatmail-iroh/              # Iroh relay supervisor
│   ├── chatmail-shadowsocks/       # Optional camouflage proxy
│   ├── chatmail-tasks/             # Background maintenance scheduler
│   ├── chatmail-metrics/           # Prometheus OpenMetrics
│   ├── chatmail-types/             # Shared errors + domain helpers
│   └── ... (tls, acme, etc.)
├── context/                        # Large read-only reference trees (git submodules or checkouts)
│   ├── madmail/                    # The original Go implementation + docs
│   ├── cmdeploy/                   # Deployer
│   ├── cmlxc/                      # Incus/LXC test runner
│   ├── stalwart/                   # Full Rust email server (inspiration/parity)
│   ├── iroh/                       # Iroh p2p (we supervise an iroh-relay)
│   ├── webrtc/ + rtc/              # WebRTC stack
│   ├── core/                       # Delta Chat core
│   └── ...
├── external/                       # Editable git submodules you actually develop
│   ├── madmail-admin-web/          # SvelteKit admin panel (newly added)
│   └── README.md
├── docs/                           # All documentation (you are here)
│   ├── project/                    # ← This step-by-step human guide
│   ├── TDD/                        # Technical Design Document (authoritative deep dives)
│   ├── plans/                      # Historical implementation tickets (b1–b9, p1, t1)
│   ├── local-dev.md
│   ├── install-simple-ip-acme.md
│   └── ...
├── data/                           # Local dev runtime state (gitignored in real use)
│   ├── chatmail.toml
│   ├── admin_token
│   ├── chatmail.db
│   ├── mail/                       # Maildir storage for test users
│   └── ...
├── target/                         # Cargo build artifacts
└── ...
```

## The Three Worlds You Must Internalize

### 1. `crates/` — The Shipping Product (Rust)

This is where 95% of active development happens for the v2 server.

- `chatmail` is the binary you `cargo run` or deploy.
- All other `chatmail-*` crates are libraries that the binary (and sometimes tests or admin tools) depend on.
- Designed for **single-binary deployment** — the admin web SPA, static docs, etc. can be compiled in via `build.rs`.

### 2. `context/` — Reference / Archaeology / Inspiration

These are **large**, often read-only or infrequently touched trees.

- `context/madmail/` — the living Go implementation that this Rust version aims to match in behavior.
  - Has excellent docs under `context/madmail/docs/`.
  - Its `internal/` directory is the spiritual ancestor of many `crates/chatmail-*` modules.
- `context/stalwart/` — a full-featured modern Rust email server. Used for ideas around protocol handling and as a "what a complete implementation looks like".
- `context/iroh/`, `context/webrtc/`, `context/rtc/` — the p2p and media stacks we integrate or supervise.
- `context/core/` — Delta Chat core (for E2E tests and understanding the client side).
- `context/cmlxc/`, `context/cmdeploy/` — testing and deployment tooling.

**Rule of thumb**: When you wonder "how did Madmail do X?", look in `context/madmail/`. When you want modern Rust inspiration for SMTP/IMAP, look at stalwart.

### 3. `external/` — Things You Edit and Ship

Currently contains the admin web SvelteKit app as a git submodule.

- You edit it, run `make build-admin-web`, then `make build-with-admin-web` (or the specific targets) to embed the built SPA into the `chatmail` binary via `chatmail-admin-web/build.rs`.
- See `external/README.md` for the exact workflow.

## Runtime Artifacts (`data/` in dev)

When you run the server locally:

- `data/chatmail.db` (or `credentials.db`) — SQLite with settings, accounts (hashes), quotas, federation stats, blocklist, etc.
- `data/admin_token` — 64-char bearer token for the Admin API (mode 0600).
- `data/mail/<user>/Maildir/...` — actual message storage (Maildir + Delta Chat folders).
- `data/certs/` — TLS material.
- `data/remote_queue/` — outbound delivery queue persistence.

These are created on first boot or via `make reset-db`.

## Important Non-Source Files

- `Makefile` — 90% of what a human runs (`make build`, `make restart`, `make test-e2e`, `make push`, etc.).
- `Cargo.toml` (root) — workspace members + pinned dependency versions.
- `.gitmodules` — declares the `external/madmail-admin-web` submodule.
- `scripts/build-release-static.sh` — produces a fully static binary (no glibc dependency surprises on target servers).

## How to Explore Efficiently

1. Start at repo root.
2. `ls crates/` to see the crate list.
3. Pick a crate, read its `Cargo.toml` (dependencies), then `src/lib.rs` (module tree).
4. For behavior questions: `context/madmail/docs/` + `docs/TDD/`.
5. For "how do I actually build/run this?": read the Makefile targets (they are well commented).

## Common Confusion Points

- "Is the binary called chatmail or madmail?" → Both. Source/binary name is `chatmail`; deployed name on many servers is `madmail`.
- "Where is the real mail server logic?" → Split across `chatmail-smtp`, `chatmail-imap`, `chatmail-fed`, `chatmail-delivery`, `chatmail-pgp`.
- "Why is there a whole `context/` tree?" → Historical reference + test harness + inspiration. Not compiled into the product by default.
- "Why embed a whole Svelte app?" → Self-contained deploys. One binary + one systemd unit = full operator experience.

## Next Step

You now know where everything lives on disk.

→ **[03-high-level-architecture.md](./03-high-level-architecture.md)**
