# Project Documentation — madmail-v2

This directory contains a **step-by-step guide** to the madmail-v2 project.

Its goal is to help you (a developer, reviewer, operator, or curious human) build a mental model of the codebase, architecture, data flows, build system, testing, and how everything fits together — without having to read thousands of lines of code first.

## Quick Start (5 minutes to orientation)

1. Read this README.
2. Read [01-introduction.md](./01-introduction.md) — what this project is and why it exists.
3. Read [02-getting-the-code.md](./02-getting-the-code.md) — layout of the repo and key directories.
4. Skim [03-high-level-architecture.md](./03-high-level-architecture.md) — the big picture and main components.
5. Jump to areas of interest (crates, flows, build, etc.).

## Documentation Map (Step-by-Step Tour)

| Step | File | What You Will Understand |
|------|------|--------------------------|
| 1 | [01-introduction.md](./01-introduction.md) | Project purpose, history (Madmail Go → Rust rewrite), goals, Delta Chat context |
| 2 | [02-getting-the-code.md](./02-getting-the-code.md) | Repository layout, submodules (context/, external/), crates/ vs context/, data/ |
| 3 | [03-high-level-architecture.md](./03-high-level-architecture.md) | Layers, runtime wiring, single-binary model, key data flows overview |
| 4 | [04-crate-by-crate-tour.md](./04-crate-by-crate-tour.md) | Every Rust crate, its responsibility, key modules, and dependency relationships |
| 5 | [05-boot-sequence-and-state.md](./05-boot-sequence-and-state.md) | `main` → `boot::run` → `ServerSupervisor`, `AppState`, hydration, flusher |
| 6 | [06-configuration-system.md](./06-configuration-system.md) | `maddy.conf` / `chatmail.toml` parsing, dynamic settings DB, effective_* helpers, CLI |
| 7 | [07-authentication-and-jit.md](./07-authentication-and-jit.md) | Login, password hashing, Just-In-Time registration, credential policy |
| 8 | [08-smtp-imap-servers.md](./08-smtp-imap-servers.md) | Custom async SMTP + IMAP implementations, sessions, IDLE, METADATA (TURN/Iroh) |
| 9 | [09-federation-inbound-outbound.md](./09-federation-inbound-outbound.md) | `/mxdeliv` HTTP federation, PGP gate, outbound queue + fallbacks (HTTP/SMTP) |
| 10 | [10-web-services-and-admin.md](./10-web-services-and-admin.md) | Public site (`chatmail-www`), registration `/new`, WebIMAP, Admin JSON-RPC + embedded SPA |
| 11 | [11-proxy-services-turn-iroh-ss.md](./11-proxy-services-turn-iroh-ss.md) | Integrated TURN/STUN, Iroh relay, optional Shadowsocks camouflage |
| 12 | [12-storage-and-persistence.md](./12-storage-and-persistence.md) | Maildir on disk, quota, in-memory caches (hot path), periodic flusher, retention |
| 13 | [13-build-test-deploy.md](./13-build-test-deploy.md) | Makefile targets, embedding admin-web, static release builds, remote push/sign, E2E testing |
| 14 | [14-understanding-context-and-references.md](./14-understanding-context-and-references.md) | `context/` (madmail Go, stalwart, iroh, webrtc, deltachat-core), external/, why they exist |
| 15 | [15-development-workflow.md](./15-development-workflow.md) | Local dev loop, `make restart`, debugging, adding features, CLI parity |
| 16 | [16-troubleshooting-and-testing.md](./16-troubleshooting-and-testing.md) | Common issues, logs, DB inspection, integration tests, Delta Chat E2E, relay-ping |
| 17 | [17-extend-and-contribute.md](./17-extend-and-contribute.md) | Where to change things, testing checklist, docs conventions |

## Two Complementary Documentation Tracks

### For Normal Users & Operators (Non-Technical / Semi-Technical)
Practical “how do I…” guides written in the same friendly, explanatory style as the original Madmail chatmail documentation:

→ **[docs/project/user-guide/](./user-guide/README.md)**

Covers: what chatmail is, quick setup, accounts & registration, privacy (PGP-only + No-Log), calls, admin tools, troubleshooting, etc.

### For Developers & People Who Want to Understand the Code
Deep, step-by-step technical tour of the architecture, crates, boot process, data flows, and internals (the series you are currently reading).

The two tracks deliberately complement each other.

## Relationship to Other Documentation

- **User & Operator Guides** (`docs/project/user-guide/`) — Practical documentation for humans running or using the servers (modeled directly on `context/madmail/docs/chatmail/`).
- **TDD/** (`docs/TDD/`) — The **Technical Design Document**. Per-topic design notes (SMTP, storage, security, etc.).
- **plans/** (`docs/plans/b1/`, `b2/`, ...) — Historical implementation steps (one `.md` per small ticket). Useful when tracing "why is this code like this?"
- **context/madmail/docs/** — The original Go Madmail implementation docs. Use for behavior parity and "how did it work before?"
- Root `Makefile` + `scripts/` — The practical build/deploy surface.
- `docs/local-dev.md` and `docs/install-simple-ip-acme.md` — Short, task-focused operator cheat sheets.

This `docs/project/` area now contains both the deep code-understanding series and the practical user/operator guides.

## Key Concepts at a Glance

- **Single binary** (`madmail`): everything (SMTP, IMAP, HTTP federation, admin API, TURN, optional proxies) in one process.
- **PGP-only by design** for Delta Chat users (enforced in SMTP DATA, APPEND, /mxdeliv).
- **Just-In-Time (JIT) registration** — accounts created on first login or `/new`.
- **Hybrid federation** — preferred HTTP POST /mxdeliv, fallback to SMTP.
- **Hot in-memory state** (`AppState`) + write-through to SQLite + Maildir.
- **Dynamic config** via DB `settings` table (most things reloadable without restart).
- **Self-contained deploys** — admin web SPA can be compiled into the binary.
- **Privacy / No-Log** and strong defaults.

## How to Use This Guide Effectively

- **First time**: Follow steps 1–5 sequentially.
- **Hacking a specific area**: Start at the crate tour, then the relevant flow doc, then jump into the TDD section and source.
- **Debugging a running server**: See boot sequence + logging + supervisor + data flows.
- **Adding a feature**: Read the crate map + the closest implementation plan + the "extend" guide.
- **Operator questions**: Focus on config, install, admin, storage, and the install-*.md files.

## Contributing to These Docs

When you change code or behavior:
- Update the relevant `docs/project/*.md` (especially the crate tour and flow documents).
- Cross-link to new TDD or plan files.
- Keep the "step by step" narrative voice friendly and progressive.

Start here: **[01-introduction.md](./01-introduction.md)**

---

*This documentation set explains the project for operators and contributors. It links to the existing TDD and plan artifacts rather than duplicating them.*
