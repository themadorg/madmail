# 14 — Understanding `context/` and `external/` (The Reference Forests)

These two directories contain the majority of the bytes in the repository, yet most of the code in them is **not** compiled into the final `chatmail` binary.

Understanding why they exist is key to not getting lost.

## `context/` — The Archaeology + Inspiration Layer

This is a collection of large, related projects that were either:
- The original implementation (Madmail Go)
- Earlier experiments (cmrelay, chatmaild)
- The tooling used to test and deploy (cmlxc, cmdeploy)
- Modern reference implementations or dependencies we integrate with (stalwart, iroh, webrtc, deltachat-core)

### Most Important Sub-Trees

**`context/madmail/`**

The living Go implementation of Madmail (the maddy fork + Chatmail patches).

- This is the behavior reference.
- Its `internal/` directory (`endpoint/`, `auth/`, `storage/`, `target/remote/`, `federationtracker/`, etc.) is the direct ancestor of many `crates/chatmail-*` modules.
- Its `docs/` directory (especially `chatmail/` and `internals/`) is still the best place to read the original design rationale.
- When a Rust implementation differs in a subtle way, the Go code + its tests are the tie-breaker.

**`context/stalwart/`**

A full-featured, modern, async Rust email server (SMTP + IMAP + JMAP + more).

- Not used at runtime.
- Used as "what does a complete, correct implementation of these protocols look like in Rust in 2025/2026?"
- The TDD documents explicitly recommend studying `stalwart/crates/smtp` and `stalwart/crates/imap` while writing the custom chatmail versions.

**`context/iroh/` + `context/webrtc/` + `context/rtc/`**

The p2p and media stacks.

- `chatmail-iroh` supervises an iroh-relay that comes from (or is compatible with) this tree.
- `chatmail-turn` uses types and ideas from the webrtc/rtc work.
- The E2E call tests (`scripts/core-e2e-turn.sh`, `tests/turn_e2e.rs`) exercise code paths that touch these stacks.

**`context/core/` (deltachat-core)**

The real Delta Chat core (C + Rust FFI).

- Used when running `make test-deltachat` so that we test against the actual client that users run, not a synthetic one.

**`context/cmlxc/` + `context/cmdeploy/`**

- `cmlxc` — thin wrapper around incus + uv for standing up disposable VMs for E2E.
- `cmdeploy` — the original deployer for Madmail (still useful for understanding production layout).

**Other context/ entries**

- `certbot/` (ACME reference)
- `lers/` (another ACME / TLS thing)
- `chatmail-turn/` (the turn-rs reference implementation)
- `relay-ping/` (the dclogin / step-by-step test tool)

## `external/` — The Editable Submodules You Actually Ship

Currently contains only:

**`external/madmail-admin-web/`**

The SvelteKit + TypeScript + Tailwind admin panel.

- This one you **do** edit and commit changes from (after `git submodule update --init`).
- The built output is what gets embedded into the `chatmail` binary.
- Upstream lives at `themadorg/madmail-admin-web`.

See `external/README.md` and the Makefile targets `build-admin-web` / `build-with-admin-web`.

## Why Not Vendor Everything or Use Cargo Dependencies?

- The Go Madmail tree is enormous and has its own build system, tests, and docs. Vendoring the whole thing as a Rust dependency would be insane.
- Stalwart, iroh, webrtc-rs, deltachat-core are all actively developed upstream. We want to track them and study them without forcing their exact versions into our Cargo workspace.
- The admin web is intentionally a separate frontend project (different language, different release cadence).

The `context/` + `external/` layout is a pragmatic compromise that keeps the shipping Rust product small while still giving developers the full reference material they need.

## How to Use These Trees Effectively

- "How did the original Madmail implement X?" → `context/madmail/internal/...` + its docs.
- "What is the correct modern Rust way to do SMTP session state?" → read the relevant parts of `context/stalwart/crates/smtp` while writing or debugging `crates/chatmail-smtp`.
- "I need to understand what a real Delta Chat client will send over IMAP for calls" → look in `context/core/` and the TURN E2E scripts.
- "I want to improve the admin UI" → work in `external/madmail-admin-web/` (with bun/npm), then `make build-with-admin-web`.

## Rule of Thumb for New Contributors

If you are touching Rust code inside `crates/`, you will spend 80% of your time in `crates/` and `docs/`.

You will only occasionally `cd context/madmail` or `cd external/madmail-admin-web` when you need the reference implementation or are changing the UI.

## Next

With the full physical layout of the repo understood, the last two documents cover the **day-to-day development workflow** and **how to extend or debug** the system.

→ [15-development-workflow.md](./15-development-workflow.md)
