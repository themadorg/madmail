# User prompt — request implementation plan for madmail-v2

Copy everything below the line into your external AI session **after** attaching:

1. **System prompt**: `docs/prompts/planing-system-prompt.md`
2. **Context bundle**: `context.txt` (generate with `./scripts/build-planning-context.sh` — ~650k–900k tokens; excludes Stalwart, `node_modules`, and Madmail `exchangers/`)

---

## Task

Using the attached **context.txt** and the system instructions, produce the **implementation plan** for **madmail-v2**.

**Critical:** Phase 1 must be planned **in full detail** (step-by-step tasks, **unit tests**, and **folder structure**). Later phases (2–10) stay at roadmap level unless noted otherwise.

## Project summary (for orientation)

We are building a **Rust** Chatmail-compatible mail server to **replace Madmail (Go)**. The new server must:

- Match **Madmail** behaviour documented in `docs/TDD/` and implemented under `context/madmail/`
- Satisfy **legacy Chatmail deployment expectations** embodied in `context/cmdeploy/` (Dovecot/Postfix configs + online pytest suite)
- Work with **Delta Chat** clients as implemented in `context/core/` (IMAP/SMTP/configure)

Key capabilities (from `docs/TDD/00-intro.md`):

- JIT / automatic user registration, PGP-only mail policy
- SMTP (25 + submission 465/587), custom async IMAP with IDLE, QUOTA, METADATA, XCHATMAIL
- HTTP federation `POST /mxdeliv` with SMTP fallback
- Federation policy (ACCEPT/REJECT), endpoint cache, in-memory federation tracker
- Admin JSON-RPC API, WebIMAP (REST + WebSocket)
- TURN + Iroh relay integration
- Filesystem mail storage + in-memory hot data (quotas, rules, users)
- SQLite settings DB, dynamic config without restart
- E2E tests via Delta Chat RPC (`context/madmail/tests/deltachat-test/`)

The repo today is a stub (`madmailv2/` with minimal `Cargo.toml`). Phase 1 establishes the real workspace layout under this tree (or a renamed root crate `madmail-v2`).

---

## What I need from you

### A. Full project plan (all phases)

1. **Phased roadmap** (6–10 phases) with durations, dependencies, TDD section mapping, Madmail/cmdeploy references, definition-of-done per phase.
2. **Crate/workspace recommendation** (names, boundaries, key dependencies).
3. **Per-subsystem breakdown** (SMTP, IMAP, storage, federation, admin, webimap, security, CLI, deploy).
4. **Test mapping** — `deltachat-test` scenarios and `cmdeploy` online tests per phase.
5. **Risks, open questions, and TDD gaps**.

### B. Phase 1 — mandatory deep dive (most important)

Plan **Phase 1 only** as an implementable sprint. Use this structure:

#### B.1 Phase 1 goal & scope

- One paragraph: what “done” means for Phase 1 (skeleton + config + SQLite settings + logging/tracing + CI; **no** SMTP/IMAP listeners yet unless you justify a minimal health endpoint).
- Explicit **in scope** / **out of scope** lists.

#### B.2 Repository & folder structure

Provide a **complete directory tree** (every folder and key files), for example:

```
madmail-v2/                    # or madmailv2/ — state your choice
├── Cargo.toml                  # workspace
├── crates/
│   ├── chatmail/               # main binary library
│   ├── chatmail-config/
│   ├── chatmail-db/
│   └── ...
├── src/ or crates/chatmail/src/
├── tests/                      # integration tests
├── migrations/
└── ...
```

For **each** directory/module, one line: **responsibility** and **which Madmail/TDD path** it will eventually mirror.

#### B.3 Phase 1 implementation steps

Numbered steps an engineer can follow in order (aim for **15–30 steps**). Each step must include:

| Field | Required |
|-------|----------|
| **Step ID** | e.g. `P1-S07` |
| **Action** | Concrete task (create file X, add dependency Y, implement trait Z) |
| **Files touched** | Paths to create or modify |
| **References** | `docs/TDD/...` or `context/madmail/...` to read |
| **Verification** | Command or test that proves the step is done |

#### B.4 Phase 1 unit tests (required)

List **every** unit test to write in Phase 1 **before** integration/E2E. For each test:

| Field | Required |
|-------|----------|
| **Test ID** | e.g. `P1-UT-03` |
| **Module path** | e.g. `crates/chatmail-config/src/settings.rs` |
| **Test name** | Rust `#[test]` or `#[tokio::test]` name |
| **What it asserts** | Behaviour in one sentence |
| **Setup** | Fixtures, temp dirs, in-memory DB |
| **Maps to step** | Which `P1-Sxx` step introduces it |
| **Madmail/TDD basis** | Why this test matters (e.g. settings DB from `settings_db.md`) |

Minimum coverage for Phase 1 unit tests (adapt with specifics):

- Config file load + env override merge
- Settings table read/write + typed getters
- Migration apply idempotency (empty DB → schema version N)
- Default values match Chatmail/Madmail expectations (PGP-only default, registration closed, etc.)
- Path resolution for data dir / admin token path
- Structured logging / tracing span on startup (smoke: no panic)
- Error types: invalid config → clear `Display` / error chain

Also specify:

- Where tests live (`#[cfg(test)]` in crate vs `tests/` integration)
- `cargo test` invocations for the phase (`cargo test -p chatmail-db`, etc.)
- Any test-only dev-dependencies (`tempfile`, `assert_matches`, etc.)

#### B.5 Phase 1 definition of done

Checklist with commands:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test --workspace
# optional: cargo test -p chatmail-db -- --nocapture
```

List exact artefacts: `config.example.toml`, `migrations/001_*.sql`, README “Phase 1” section, etc.

---

## Priority order

When trade-offs appear, prioritize in this order:

1. Correctness vs real Delta Chat + cmdeploy online tests (later phases)  
2. Security defaults in config/schema (even in Phase 1)  
3. Feature parity with Madmail  
4. Operational simplicity (single binary path, SQLite, documented layout)  
5. Performance (after parity)

## Reference map (start here in context)

| Area | Where to look in context |
|------|--------------------------|
| New design | `docs/TDD/00-intro.md`, `01-architecture.md`, `13-configuration.md` (if present), `17-data-models.md` (if present) |
| Madmail settings/DB | `context/madmail/docs/chatmail/settings_db.md`, `internals/database.md` |
| Madmail code (later ports) | `context/madmail/internal/db/`, `config.go`, `directories.go` |
| Testing philosophy | `docs/TDD/16-testing.md` |
| Legacy stack (later) | `context/cmdeploy/src/cmdeploy/dovecot/`, `tests/online/` |

Please read the full context before answering. If the bundle is truncated, say which sections you lack and still produce the best plan possible from what you have.

**Do not** skip Section B (Phase 1 detail). A high-level roadmap without Phase 1 steps, unit tests, and folder tree is not acceptable.
