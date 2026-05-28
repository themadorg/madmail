# Phase 1 — Skeleton, config, DB, logging

Phase 1 delivers a bootable `chatmail` binary with no network listeners. Implementation is tracked as **one markdown file per step** under [`docs/plans/b1/`](plans/b1/README.md).

## Goal

- Parse CLI and static config (TOML + Madmail `.conf`)
- Open SQLite (`chatmail.db`), run embedded migrations
- Dynamic settings DAO (`settings` table)
- Reloadable `tracing` with No-Log policy (`__LOG_DISABLED__`)
- Persist admin API token at `admin_token` (mode `0600`)

## Step index

See [docs/plans/b1/README.md](plans/b1/README.md).

## Design docs (TDD)

| Topic | Document |
|-------|----------|
| Overview | [TDD/00-intro.md](TDD/00-intro.md) |
| Architecture | [TDD/01-architecture.md](TDD/01-architecture.md) |
| Security / No-Log | [TDD/12-security.md](TDD/12-security.md) |
| Admin API (token) | [TDD/09-admin-api.md](TDD/09-admin-api.md) |
| Storage (later) | [TDD/04-storage-layer.md](TDD/04-storage-layer.md) |
| Testing | [TDD/16-testing.md](TDD/16-testing.md) |
| RFC index | [TDD/RFC/README.md](TDD/RFC/README.md) |

## Definition of done

- [x] `cargo fmt --check` / `cargo clippy --workspace -- -D warnings`
- [x] `cargo test --workspace`
- [x] `cargo run -p chatmail -- --state-dir ./data` creates DB + `admin_token`
- [x] Schema includes `settings`, `quotas`, `blocked_users`, `registration_tokens`, `dns_overrides`
- [x] CI workflow green
- [x] [local-dev.md](local-dev.md) documents dev workflow

## Out of scope (Phase 2+)

SMTP/IMAP listeners, auth, maildir, federation — see [TDD/02-smtp-server.md](TDD/02-smtp-server.md), [TDD/03-imap-server.md](TDD/03-imap-server.md), [TDD/07-federation.md](TDD/07-federation.md).
