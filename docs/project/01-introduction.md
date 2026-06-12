# 01 — Introduction: What is madmail-v2?

## The Big Picture

**madmail** (this project; workspace crate `chatmail`, binary `madmail`) is a privacy-oriented federated email server for **Delta Chat** users. It implements the **chatmail** relay protocol.

It is a **Rust rewrite** of the original Madmail (a fork of the Go `maddy` mail server with Chatmail-specific changes).

- Original (Go): `context/madmail/` + `internal/` (endpoints, auth, storage, etc.)
- Rust rewrite: `crates/chatmail*` (the focus of this repo)

The goal of the rewrite is:
- Memory safety and async I/O (Tokio)
- Easier auditing and long-term maintainability
- Single-binary distribution and cross-compilation
- WebSocket and embedded SPA for web/admin surfaces
- Operational experience aligned with Madmail

## Core Philosophy (inherited from Chatmail / Madmail)

1. **Automatic / JIT registration** — users get accounts on first login or via `/new`. No manual admin approval for basic use.
2. **Strict PGP-only policy** — almost all mail must be encrypted (Delta Chat uses Autocrypt + SecureJoin). Plaintext is rejected except for specific handshakes and bounces.
3. **HTTP-based federation preferred** (`POST /mxdeliv`) with SMTP fallback. This gives direct inter-server delivery with SMTP as a fallback.
4. **Strong privacy defaults** — No-Log mode, minimal logging, no plaintext storage of sensitive data.
5. **Built-in real-time support** — TURN server for Delta Chat voice/video calls (WebRTC), Iroh relay for WebXDC / p2p data.
6. **Operator tooling** — single binary, Madmail-compatible CLI (`madmail ...`), JSON-RPC admin API, optional embedded Svelte admin web UI.
7. **Stealth / camouflage deployment** possible (Shadowsocks proxy mode).

## Why Delta Chat Needs This

Delta Chat turns email into a secure messenger using:
- IMAP + SMTP as the transport
- PGP (Autocrypt) for E2E encryption
- SecureJoin for contact verification / key exchange

Traditional email servers (Postfix + Dovecot, etc.) are:
- Complex to operate
- Not optimized for the "always encrypted + small messages + high reliability" pattern
- Missing first-class support for TURN/Iroh metadata discovery via IMAP METADATA

Chatmail servers (Madmail + this Rust port) are aimed at this use case rather than general-purpose mail hosting.

## History and Lineage

- **maddy** — clean, modular Go mail server (https://github.com/foxcpp/maddy)
- **Madmail** (themadorg) — fork + heavy patches for Chatmail (JIT, PGP gate, /mxdeliv federation, TURN, No-Log, admin UI, etc.). Lives in `context/madmail/`
- **cmrelay / chatmaild** (Python reference) — earlier experiments
- **madmail-v2** — this project. Rust implementation aiming for Madmail feature parity.

The project is implemented in phases (see `docs/plans/b1/` through `b9/`, then `p1/`).

## Current Status (as of 2026)

- Core SMTP + IMAP + federation + auth + storage + admin API implemented and tested.
- TURN + Iroh + Shadowsocks integration done.
- Admin web SPA (SvelteKit from `external/madmail-admin-web`) can be built and embedded.
- E2E tests with real Delta Chat clients (desktop + core) passing in CI-like setups (incus).
- Static release binaries for easy deployment on Debian and similar.
- CLI parity with Madmail is in progress (many commands implemented; some still stubs).

## Naming: chatmail vs madmail

- **Chatmail** — the relay protocol and server *concept* (JIT accounts, PGP-only, HTTP federation, TURN metadata, etc.). Any server that speaks this protocol is a *chatmail relay* (madmail, cmdeploy stacks, etc.).
- **madmail** — this project and its CLI tool / binary (`target/release/madmail`, typically installed as `/usr/local/bin/madmail`).
- **`chatmail` crate** — the main Rust workspace member under `crates/chatmail/` (library + binary entry point). Crate names like `chatmail-smtp` are internal module names, not the operator-facing tool.
- Config files historically used `maddy.conf` syntax; the Rust version also accepts `chatmail.toml` (filename unchanged).

## Next Step

Now that you know *why* this exists, let's look at the actual files on disk.

→ **[02-getting-the-code.md](./02-getting-the-code.md)**
