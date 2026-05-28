# 11 — Proxy & Sidecar Services (TURN, Iroh, Shadowsocks)

One of the things that makes a modern chatmail server special is that it ships with the auxiliary services that Delta Chat needs for real-time and p2p features.

These run in the same process (or as supervised children) and are configured via the same mechanisms as the mail protocols.

## TURN / STUN (`chatmail-turn`)

### Purpose

WebRTC media relay for Delta Chat voice and video calls (and sometimes file transfer).

When two Delta Chat clients want to do a call:
- They perform ICE negotiation.
- If they cannot do direct p2p, they need a TURN server that both can reach.
- The chatmail server the users are on provides that TURN server.

### How It Is Integrated

- `chatmail-turn` crate wraps the actual TURN server implementation (from the webrtc-rs / turn-rs work in `context/`).
- Started in `turn_boot.rs` at supervisor startup (conditional on `turn_configured()` in static config + admin `__TURN_ENABLED__` toggle).
- Credentials and server address are **not** distributed via DNS or a separate API.
- They are served to authenticated clients via the IMAP `GETMETADATA` extension (see the IMAP document).

This means:
- Only users who can log into the IMAP account learn the TURN secret.
- The same server that holds the mail also provides the media relay (simple for operators, good for trust).

### Discovery Objects

`TurnDiscovery` struct (in the turn crate) contains:
- Whether TURN is active
- Host / port
- Shared secret
- TTL
- Test/force-relay flag (for QA)

This object is put into `ImapSessionConfig` and surfaces in the METADATA responses.

### Testing

There is an entire phase (b9) and `docs/plans/b9/` dedicated to TURN.

- Smoke tests for STUN binding and TURN allocation
- Dual allocation tests
- E2E tests that actually make a call through the relay (`tests/turn_e2e.rs`)
- `scripts/core-e2e-turn.sh`

## Iroh Relay (`chatmail-iroh`)

### Purpose

Iroh (https://iroh.computer) is a modern p2p / hole-punching library used by Delta Chat for:
- WebXDC (interactive web apps inside chats)
- Faster / more reliable blob transfer
- Future p2p experiments

The chatmail server can run (or supervise) an `iroh-relay` instance so that clients behind difficult NATs still have a reliable rendezvous point that is operated by the same people who run their mail server.

### Integration

- `chatmail-iroh` crate
- Started in `iroh_boot.rs`
- It can either embed the relay logic or supervise a separate `iroh-relay` binary (downloaded via `make init` in dev setups).
- Discovery info is again surfaced via IMAP METADATA, parallel to TURN.

### Assets

The crate has a small `assets/` directory with a VERSION file used at build time.

## Shadowsocks (`chatmail-shadowsocks`)

### Purpose

Optional "camouflage" / stealth mode.

In censored networks, running a mail server on ports 25/143/443 can be risky or blocked.

The Shadowsocks integration lets the same ports (or additional ones) speak the Shadowsocks proxy protocol. To a network observer it looks like the user is just using a normal SOCKS/Shadowsocks proxy to the server, not running a mail/chat service.

### Integration

- `chatmail-shadowsocks` crate
- Started in `ss_boot.rs` when enabled
- `allowed_ports.rs` controls which real mail ports are exposed through the proxy
- Admin can toggle via settings


## Configuration & Runtime Control

All three services follow the same pattern:

1. Declared in static config (`turn_*`, `iroh_*`, `ss_*` blocks or keys).
2. Can be toggled at runtime via the admin API / settings table (`__TURN_ENABLED__`, etc.).
3. On toggle or reload, the supervisor can start or stop the sidecar.
4. Discovery information for TURN/Iroh is re-read from the DB + config on IMAP session creation (so clients see the change reasonably quickly).

## Why Bundle Them?

- **Operational simplicity** — one binary, one service file, one set of TLS certs, one admin UI.
- **Trust & privacy** — the same operator who hosts the encrypted mail also hosts the TURN/Iroh relay. No need to send ICE credentials to a third-party service.
- **Discovery** — IMAP METADATA is already authenticated and encrypted (over TLS + login). Perfect channel for distributing the relay endpoints and secrets.
- **Testing** — E2E tests can stand up the full stack (mail + TURN + Iroh) in one process or one container.

## Trade-offs

- If the mail server process OOMs or crashes, the TURN relay goes down too (mitigated by supervisor restarts + systemd).
- Resource usage: TURN can be bandwidth-heavy during calls. Operators need to monitor.
- The bundled TURN is not intended to be a public open relay for arbitrary WebRTC traffic — only for the accounts on that server.

## Next

With the sidecars understood, the last core piece is **how mail is actually stored and how the hot in-memory state stays consistent with disk**.

→ [12-storage-and-persistence.md](./12-storage-and-persistence.md)
