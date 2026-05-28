# P9-S03: Config — IMAP turn_* and turn { } block

## Action

Extend `chatmail-config` / `maddy.conf` parser:

| Directive | `AppConfig` field |
|-----------|-------------------|
| `turn_enable` | `turn_enable` |
| `turn_server` | `turn_server` |
| `turn_port` | `turn_port` (default 3478) |
| `turn_secret` | `turn_secret` |
| `turn_ttl` | `turn_ttl` (default 86400) |
| `turn udp://… tcp://… { realm secret relay_ip }` | `turn_listen`, `turn_realm`, … |

Map `$(public_ip)` for discovery host and `external` on turn-rs.

## Files touched

- `crates/chatmail-config/src/maddy.rs`
- `crates/chatmail-config/src/lib.rs`
- `crates/chatmail-config/tests/turn_config.rs`

## TDD references

- [13-configuration.md](../../TDD/13-configuration.md)
- [11-proxy-services.md](../../TDD/11-proxy-services.md)

## Madmail / context references

- `context/madmail/maddy.conf` — IMAP + turn blocks
- `context/madmail/internal/cli/ctl/maddy.conf.j2`

## RFC references

_None on wire; operational config only._

## Verification

**P9-UT03**

```bash
cargo test -p chatmail-config turn
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P9-UT03** | `P9-S03` |

## Next

[P9-S04-imap-getmetadata-turn.md](P9-S04-imap-getmetadata-turn.md)
