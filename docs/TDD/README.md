# Technical Design Document — Rust Chatmail Mailserver

This directory contains the Technical Design Document (TDD) for **madmail-v2**, a Rust implementation of the Chatmail federated mail server. The running binary and root crate are named **`chatmail`** (`crates/chatmail/`).

## Workspace crates

Twenty-one library crates under `crates/` plus integration tests in `tests/`. Full dependency diagram, runtime wiring, and per-section mapping: **[01-architecture.md](01-architecture.md#rust-workspace-crates)**.

| Crate | TDD topics |
|-------|------------|
| `chatmail` | Boot, supervisor, CLI |
| `chatmail-smtp` / `chatmail-imap` | [02](02-smtp-server.md), [03](03-imap-server.md) |
| `chatmail-fed` / `chatmail-delivery` / `chatmail-pgp` | [07](07-federation.md), [12](12-security.md) |
| `chatmail-storage` / `chatmail-state` / `chatmail-db` | [04](04-storage-layer.md), [17](17-data-models.md) |
| `chatmail-auth` | [05](05-authentication.md) |
| `chatmail-www` | [10](10-webimap.md) |
| `chatmail-admin` / `chatmail-admin-web` | [09](09-admin-api.md) |
| `chatmail-turn` / `chatmail-iroh` / `chatmail-shadowsocks` | [11](11-proxy-services.md), [20](20-deltachat-calls.md) |
| `chatmail-push` | [23](23-push-notifications.md) |
| `chatmail-config` / `chatmail-tasks` | [13](13-configuration.md), [14](14-cli-tools.md), [21](21-scheduled-maintenance.md) |
| `chatmail-acme` / `chatmail-tls` | [19](19-certificates.md) |
| `chatmail-metrics` | OpenMetrics (see [16-testing.md](16-testing.md)) |

## Document structure

| File | Description |
|------|-------------|
| `README.md` | This file — navigation and crate index |
| `00-intro.md` | Project goals, scope, and principles |
| `01-architecture.md` | Components, data flow, **workspace layout** |
| `02-smtp-server.md` | SMTP (`chatmail-smtp`) |
| `03-imap-server.md` | IMAP (`chatmail-imap`) |
| `04-storage-layer.md` | Maildir + in-memory hot data |
| `05-authentication.md` | JIT registration and auth |
| `07-federation.md` | HTTP `/mxdeliv` and outbound delivery |
| `09-admin-api.md` | Admin RPC (`chatmail-admin`) |
| `10-webimap.md` | WebIMAP REST + WebSocket + WebSMTP |
| `11-proxy-services.md` | TURN, Iroh, Shadowsocks |
| `12-security.md` | PGP-only, No-Log, federation policy, TLS |
| `13-configuration.md` | `maddy.conf` + settings DB |
| `14-cli-tools.md` | CLI subcommands — Madmail parity matrix; links [`../guide/cli/`](../guide/cli/README.md) |
| `16-testing.md` | Unit, integration, Delta Chat E2E |
| `17-data-models.md` | SQLite schema |
| `19-certificates.md` | TLS: install modes, ACME |
| [`../install-simple-ip-acme.md`](../install-simple-ip-acme.md) | Operator guide: `--simple --ip --auto-ip-cert` |
| `20-deltachat-calls.md` | Calls ICE/TURN test matrix |
| `21-scheduled-maintenance.md` | Retention, dormant accounts (`chatmail-tasks`) |
| `22-bandwidth-monitoring.md` | Bandwidth spec (planned) |
| `23-push-notifications.md` | XDELTAPUSH, `notifications.delta.chat`, modes, CLI `madmail push` |

## RFC reference library

Implementation-related IETF RFCs are stored as **plain text** under [`RFC/`](RFC/) (46 files: `rfc*.txt` and `draft-uberti-behave-turn-rest-00.txt`).

- **Index and per-section mapping:** [`RFC/README.md`](RFC/README.md)
- **Regenerate copies:** `docs/TDD/RFC/download-rfcs.sh`
- **In each TDD file:** see the **Related RFCs** section at the end (IETF Datatracker link + local `RFC/rfc….txt` path)

## Implementation reference codebases

Example implementations live in `context/` (madmail, cmrelay, cmdeploy, stalwart). See [`CONTEXT.md`](CONTEXT.md) for the full path index; each TDD section links the relevant files.

## Implementation plans

Phase sprint plans (each step = one file, with TDD/RFC links):

| Phase | Plan folder |
|-------|-------------|
| 1 | [`../plans/b1/`](../plans/b1/README.md) |
| 2 | [`../plans/b2/`](../plans/b2/README.md) |
| 3 | [`../plans/b3/`](../plans/b3/README.md) |
| 4 | [`../plans/b4/`](../plans/b4/README.md) |
| 5 | [`../plans/b5/`](../plans/b5/README.md) |
| 6 | [`../plans/b6/`](../plans/b6/README.md) |
| 7 | [`../plans/b7/`](../plans/b7/README.md) |
| 8 | [`../plans/b8/`](../plans/b8/README.md) |
| 9 | [`../plans/b9/`](../plans/b9/README.md) — TURN/STUN |

Regenerate step files: `python3 scripts/generate-phase-plans.py`

## Operator CLI reference

Per-command usage (flags, examples, JSON output): **[`../guide/cli/README.md`](../guide/cli/README.md)**. The TDD CLI section [`14-cli-tools.md`](14-cli-tools.md) tracks **implementation parity** and maps commands to `crates/chatmail/src/ctl/`.

## How to use this document

1. Start with `00-intro.md` for context.
2. Read `01-architecture.md` for system shape and the **`crates/` map**.
3. Dive into specific areas as needed during implementation.
4. Operators: use [`../guide/cli/`](../guide/cli/README.md) for day-to-day `madmail` commands.
5. When adding or splitting crates, update `01-architecture.md` and this README.

## Contributing

When implementing a feature, update the corresponding section and note the owning crate(s), chosen dependencies, and any deviation from Madmail.

## Status

Design document updated as implementation progresses. Core protocol sections (SMTP, IMAP, federation, admin, storage, auth, proxies, push) map to implemented crates. [04-storage-layer.md](04-storage-layer.md) documents Maildir + CAS blobs, `chatmail-uidlist`, and `mail_fsync`/`blob_dedup` policy. [23-push-notifications.md](23-push-notifications.md) documents XDELTAPUSH + `notifications.delta.chat` (default off). [22-bandwidth-monitoring.md](22-bandwidth-monitoring.md) is specification-only until `chatmail-state` gains counters.

**Target**: Feature parity with Madmail (Go), implemented as a Rust workspace.
