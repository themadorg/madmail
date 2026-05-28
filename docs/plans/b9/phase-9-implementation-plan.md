# Phase 9 — Implementation plan (index)

Engineering backlog for **TURN/STUN** — one file per step. Normative design: [TDD 11-proxy-services.md](../../TDD/11-proxy-services.md).

| Step | Document |
|------|----------|
| Overview | [README.md](README.md) |
| P9-S01 … P9-S10 | Linked from README steps table |

## Architecture (target)

```
chatmail binary
├── chatmail-config     ← turn_enable, turn_secret, turn { } listen addrs
├── chatmail-imap       ← GETMETADATA "" /shared/vendor/deltachat/turn  (RFC 5464)
├── chatmail-turn       ← turn_metadata_line(); optional turn-rs runner
└── turn-rs (embedded)  ← static-auth-secret = turn_secret; realm = public_ip
```

## RFC-driven acceptance

| Requirement | RFC / draft |
|-------------|-------------|
| Advertise `METADATA` when TURN enabled | [RFC 5464](https://datatracker.ietf.org/doc/html/rfc5464) |
| Metadata value `host:port:user:pass` | Chatmail convention + [TURN REST draft §2.2](https://datatracker.ietf.org/doc/html/draft-uberti-behave-turn-rest-00#section-2.2) |
| STUN Binding works | [RFC 8489](https://datatracker.ietf.org/doc/html/rfc8489) |
| TURN Allocate + relay | [RFC 8656](https://datatracker.ietf.org/doc/html/rfc8656) |
| Core ICE JSON | [RFC 8445](https://datatracker.ietf.org/doc/html/rfc8445) usage via core API |

## Testing strategy (three layers)

### 1. Unit tests

- Pure Rust: no network, frozen clock for HMAC username.
- Crates: `chatmail-turn`, `chatmail-config`, `chatmail-imap` (metadata handler).

### 2. Smoke tests

- Ephemeral UDP/TCP listeners on `127.0.0.1:0`.
- Minimal STUN client in test code OR turn-rs internal APIs.
- Prove Allocate succeeds with password derived from same secret as IMAP.

### 3. E2E tests

**A. In-process mail stack** (same pattern as [`tests/imap_e2e.rs`](../../../tests/imap_e2e.rs), [`tests/deltachat_p2p_e2e.rs`](../../../tests/deltachat_p2p_e2e.rs)):

- [`tests/support/mod.rs`](../../../tests/support/mod.rs) `spawn_mail_servers` extended with TURN.
- Raw TCP IMAP client [`tests/support/imap_client.rs`](../../../tests/support/imap_client.rs) — mirrors [`context/relay-ping/internal/check/imapcheck`](../../../context/relay-ping/internal/check/imapcheck/imapcheck.go) wire dialog.

**B. relay-ping** (optional against `make run-bg`):

- Build: `make relay-ping-build`
- Probe logged-in IMAP + metadata when Chatmail exposes it (future step in relay-ping).

**C. Delta Chat core** (same as [`scripts/core-e2e.sh`](../../../scripts/core-e2e.sh)):

- Spawn chatmail with TURN; run core test that calls `ice_servers()` and asserts no fallback hosts.
- Path: [`context/core/src/tests/chatmail_transport.rs`](../../../context/core/src/tests/chatmail_transport.rs).

## CI gates

```bash
cargo test -p chatmail-turn
cargo test -p chatmail-imap p9_ut04
cargo test -p chatmail-integration turn_
# optional nightly:
scripts/core-e2e-turn.sh
```

## Out of scope (phase 9)

- TURNS / `turns:` URLs ([RFC 8314](https://datatracker.ietf.org/doc/html/rfc8314) TLS) — follow-up phase
- TCP relay ([RFC 6062](https://datatracker.ietf.org/doc/html/rfc6062))
- Automated two-party video call in CI (document manual checklist)
