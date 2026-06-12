# Technical Design Document: Rust-based Chatmail Mailserver

## Project Name
**madmail-v2**

## Overview
This document outlines the technical design for a **Rust implementation** of a Chatmail-compatible mail server.

Chatmail is a privacy-oriented, federated email system for **Delta Chat** users. It emphasizes:
- Automatic / JIT user registration
- Strict **PGP-only** message policy
- HTTP-based federation (`/mxdeliv`) with SMTP fallback
- Strong privacy (No-Log policy)
- Built-in support for real-time features (TURN for calls, Iroh for WebXDC)
- Admin-friendly management via API + Web UI
- Camouflage / stealth deployment options

The goal of this Rust rewrite is to provide:
- Memory safety and async I/O (Tokio)
- Easier auditing and contribution (Rust ecosystem)
- WebSocket support for WebIMAP and admin surfaces
- Single-binary distribution and cross-compilation
- Long-term maintainability

## Goals
- **Feature Parity** with the existing Madmail (Go) implementation
- Full support for **SMTP** (submission + incoming) and **IMAP**
- **Federation** via HTTP (`/mxdeliv`) + SMTP fallback
- **Admin API** (JSON-RPC style over single endpoint)
- **WebIMAP** (REST + WebSocket) for web clients and Delta Chat desktop
- Dynamic configuration via database (no restart for most changes)
- **E2E tests** using Delta Chat RPC client where applicable
- Support for **TURN** and **Iroh Relay**
- **Quota** management, blocklist, federation policy (ACCEPT/REJECT)
- **No-Log** mode and strict PGP enforcement

## Non-Goals (Phase 1)
- Full Dovecot compatibility / SASL proxying (future)
- PostgreSQL as primary backend (SQLite first, Postgres later)
- Web admin dashboard (Svelte) — can be reused or reimplemented in Leptos/Yew

## Target Users
- Operators running Chatmail instances (especially in restricted networks)
- Delta Chat power users and developers
- Organizations needing self-hosted, auditable, privacy-focused mail infrastructure

## Key Design Principles
1. **Single-binary deployment** (like Madmail)
2. **Async-first** with Tokio + Rustls
3. **Database-backed dynamic config** (settings table)
4. **Memory-first hot paths** (federation rules, quotas, endpoint cache in RAM + sync writes)
5. **Strong defaults**: PGP-only, registration closed by default, No-Log off by default
6. **Testability**: E2E tests using real Delta Chat clients where feasible

## Document Structure
This TDD is organized into numbered sections:

- `00-intro.md` — This file
- `01-architecture.md` — High-level system architecture and **Rust workspace crate map** (`crates/`)
- `02-smtp-server.md` — SMTP listener, submission, delivery pipeline
- `03-imap-server.md` — IMAP implementation and extensions
- `04-storage-layer.md` — Filesystem mail storage + In-memory hot data (high throughput)
- `05-authentication.md` — JIT registration, password handling
- `07-federation.md` — HTTP `/mxdeliv` protocol and delivery (policy in `chatmail-state` + `chatmail-db`)
- `09-admin-api.md` — Admin RPC API design
- `10-webimap.md` — WebIMAP (REST + WebSocket)
- `11-proxy-services.md` — TURN, Iroh, Shadowsocks camouflage proxy
- `12-security.md` — PGP enforcement, No-Log, rate limiting, TLS
- `13-configuration.md` — `maddy.conf` + settings database (Madmail-compatible)
- `14-cli-tools.md` — Command-line interface
- `16-testing.md` — Unit + E2E testing strategy (`tests/` workspace + crate tests)
- `17-data-models.md` — SQLite schema (Madmail `internal/db` alignment)
- `19-certificates.md` — TLS and ACME (`chatmail-acme`, `chatmail-tls`)
- `20-deltachat-calls.md` — ICE/TURN test matrix
- `21-scheduled-maintenance.md` — `chatmail-tasks` scheduler
- `22-bandwidth-monitoring.md` — Spec (not yet implemented in `chatmail-state`)

## Implementation references

Full index: [`CONTEXT.md`](CONTEXT.md). Paths are under `../../context/`.

| Codebase | Use for this section |
|----------|----------------------|
| [madmail](../../context/madmail/) | **Primary target** — feature set madmail-v2 must match |
| [cmrelay](../../context/cmrelay/) | Prior Rust/Python relay on Dovecot; useful for `/mxdeliv` and install layout |
| [cmdeploy](../../context/cmdeploy/) | How Chatmail is deployed today (Postfix + Dovecot) and online test expectations |
| [stalwart](../../context/stalwart/) | Modern Rust MTA design patterns (SMTP/IMAP split, async sessions) |

| Topic | Example paths |
|-------|----------------|
| Server entry | [`madmail/maddy.go`](../../context/madmail/maddy.go) |
| Product docs | [`madmail/docs/chatmail/`](../../context/madmail/docs/chatmail/) |
| cmrelay layout | [`cmrelay/doc/index.md`](../../context/cmrelay/doc/index.md) |
| Deploy overview | [`cmdeploy/src/cmdeploy/cmdeploy.py`](../../context/cmdeploy/src/cmdeploy/cmdeploy.py) |

## Related RFCs

Core protocols this project implements. **Offline plain-text copies** live under [`RFC/`](RFC/) — see [`RFC/README.md`](RFC/README.md) for the full inventory (46 files) and per-section mapping. Regenerate: [`RFC/download-rfcs.sh`](RFC/download-rfcs.sh).

| RFC | Title | Local |
|-----|-------|-------|
| [5321](https://datatracker.ietf.org/doc/html/rfc5321) | Simple Mail Transfer Protocol | [rfc5321.txt](RFC/rfc5321.txt) |
| [5322](https://datatracker.ietf.org/doc/html/rfc5322) | Internet Message Format | [rfc5322.txt](RFC/rfc5322.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP4rev1 | [rfc3501.txt](RFC/rfc3501.txt) |

## Status
This design document is updated as implementation progresses. Sections will be expanded over time.

**Initial Target**: Feature-complete MVP matching Madmail core functionality within 6–9 months (community driven).