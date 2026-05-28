# P9-S05: Embed turn-rs TURN server

## Action

Wire **turn-rs** into chatmail boot:

- Path dependency: `turn-server = { path = "../../context/turn-rs" }` (or git pin).
- Generate `turn-server.toml` from `AppConfig` at runtime:
  - `[auth] static-auth-secret = turn_secret`
  - `[[server.interfaces]]` UDP+TCP on ephemeral or configured port
  - `external = turn_server` / `public_ip`
  - `realm = turn_realm`
  - `port-range = "49152..65535"`
- Start in `chatmail` main alongside IMAP/SMTP; shutdown on SIGINT.
- Test helper: `spawn_turn_server(secret, port) -> SocketAddr` for integration tests.

## Files touched

- `crates/chatmail-turn/src/runner.rs`
- `crates/chatmail/src/main.rs` (or boot module)
- `Cargo.toml` workspace deps

## TDD references

- [11-proxy-services.md](../../TDD/11-proxy-services.md) — integration options
- [01-architecture.md](../../TDD/01-architecture.md) — sidecar diagram

## Madmail / context references

- `context/madmail/internal/endpoint/turn/turn.go` — pion lifecycle
- `context/turn-rs/AGENTS.md`, `turn-server.toml`

## RFC references

- [RFC 8656](../../TDD/RFC/rfc8656.txt) — server role
- [RFC 8489](../../TDD/RFC/rfc8489.txt) — STUN on same port

## Verification

```bash
cargo check -p chatmail
cargo test -p chatmail-turn spawn_turn
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P9-SM01** | `P9-S05` (enables smoke) |

## Next

[P9-S06-smoke-stun-binding.md](P9-S06-smoke-stun-binding.md)
