# 17 — Extending the Project & Contribution Guide

You have read the step-by-step tour. Now you want to change something.

## First Principles

1. **The TDD documents (`docs/TDD/`) are the design source of truth.** If you are adding a significant feature, start by updating (or creating) the relevant TDD section.
2. **The implementation plans (`docs/plans/`) show the historical granularity.** New work can be proposed as a similar set of small, reviewable `.md` steps if the change is large.
3. **Behavior compatibility with the Go Madmail** (in `context/madmail/`) is usually required for federation and client interop. When in doubt, make the Rust version do what the Go version does (or document the intentional difference).
4. **Single-binary + simple ops** is a core constraint. Avoid adding new mandatory external processes or complex deployment steps.

## Where to Make Common Changes

| Change Type                        | Primary Locations |
|------------------------------------|-------------------|
| New CLI command (madmail parity)   | `chatmail-config/src/cli.rs`, `chatmail/src/ctl/<new>.rs`, `ctl/dispatch.rs` |
| New admin API method / resource    | `chatmail-admin/src/resources/<new>.rs`, `handler.rs` |
| New setting / dynamic toggle       | `chatmail-db/src/settings_keys.rs`, `chatmail-db/src/settings.rs`, relevant cache |
| SMTP extension or verb             | `chatmail-smtp/src/session.rs` + protocol bits |
| IMAP extension or METADATA entry   | `chatmail-imap/src/session.rs` + config passed from supervisor |
| New federation policy or routing rule | `chatmail-fed/src/mxdeliv.rs`, `chatmail-delivery/src/router.rs`, `chatmail-state` caches |
| New background maintenance job     | `chatmail-tasks/src/jobs.rs`, scheduler |
| Change to quota / storage accounting | `chatmail-state/src/quota.rs`, `chatmail-storage` |
| Change to PGP enforcement          | `chatmail-pgp/src/lib.rs` (very careful — tests are mandatory) |
| TURN / Iroh discovery change       | `chatmail-turn`, `chatmail-iroh`, `turn_boot.rs` / `iroh_boot.rs`, IMAP METADATA path |
| Admin web UI change                | `external/madmail-admin-web/` (then `make build-with-admin-web`) |
| New public web page or API         | `chatmail-www/src/handlers.rs`, `router.rs`, `webimap*` |
| Config / effective_* logic         | `chatmail-config` (the effective_* functions are the contract) |
| New migration or schema change     | `chatmail-db/migrations/sqlite/` (and postgres/), `schema.rs` |

## Adding Tests

- Every new hot-path or security-sensitive function should have a unit test next to it.
- Protocol changes should have both unit parsing tests and an E2E exercise in `tests/`.
- If the change affects registration, delivery, or federation, add or extend a Delta Chat client E2E scenario if possible.
- Update the relevant row in `docs/TDD/16-testing.md` if you are expanding coverage.

## Documentation Obligations

When you land a change that affects humans or other developers:

- Update the matching `docs/project/NN-*.md` (especially the crate tour and the flow documents).
- If it's a design-level change, update the TDD section.
- If it changes operator-visible behavior, consider updating `docs/install-*.md` or adding a note in the admin docs.
- Cross-link from the implementation plan (if you created one) back to the code.

## Commit & PR Hygiene

- Small, reviewable commits are preferred (the original b1–b9 plans were literally one small `.md` + implementation per step).
- Run `make fmt`, `make lint`, and the relevant test subset before opening a PR.
- For large features, consider opening a design / plan document first (similar to the existing `plans/` entries).
- Never commit `target/`, `node_modules/`, or your personal `.env` / `data/`.

## Working with the Submodules

- `external/madmail-admin-web` — treat it like a normal repo for UI work. The parent repo just records the commit pointer.
- `context/*` trees — you almost never commit changes to them. They are reference material. If you find a bug in one of them while studying, open an issue or PR against the upstream project, not here.

## Security & Privacy Changes

- Anything touching the PGP gate, auth, admin token handling, or logging (No-Log) is high risk.
- Such changes require:
  - Unit + E2E tests that specifically exercise the security property.
  - Careful review of error messages (they must not leak information).
  - Usually an update to `docs/TDD/12-security.md`.

## When to Add a New Crate

Only when the concern is clearly separable and likely to be used by multiple other crates or external tools.

Examples of good new crates: a new sidecar supervisor, a new storage backend abstraction, a metrics collector.

Do **not** create a new crate just to split a 400-line file.

## Getting Help / Asking Questions

- The existing TDD and plan documents are the primary written record.
- The Go Madmail source + its docs are the behavioral spec.
- The integration tests in `tests/` are executable documentation of "what good looks like."
- For real-time discussion, the project uses whatever chat channels the team has (often Delta Chat itself on a madmail server).

## Final Encouragement

This project was built by following a very detailed, step-by-step plan across many phases. The same approach works for new work:

1. Write a small plan (or a TDD update).
2. Implement the smallest possible vertical slice.
3. Add tests that would have caught the thing you just built.
4. Document it in the human-facing `docs/project/` series.
5. Ship.

Welcome to the codebase. You now know where everything is and how it fits together.

— The madmail-v2 documentation team
