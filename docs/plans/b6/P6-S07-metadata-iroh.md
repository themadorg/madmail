# P6-S07: GETMETADATA iroh

## Action

Advertise `/shared/vendor/deltachat/irohrelay` with the operator relay URL; run **iroh-relay v0.35.0** under `chatmail` supervision (embedded binary, not a separate systemd unit).

## Files touched

- `crates/chatmail-iroh/` — embed + spawn relay
- `crates/chatmail/src/iroh_boot.rs`, `supervisor.rs`
- `crates/chatmail-imap/src/session.rs` — METADATA key
- `crates/chatmail-config` — `iroh_relay_url` in `maddy.conf`
- `crates/chatmail-admin` — `/admin/services/iroh` soft reload

## TDD references

- [03-imap-server.md](../../TDD/03-imap-server.md)
- [11-proxy-services.md](../../TDD/11-proxy-services.md) — § Iroh relay

## Madmail / context references

- [`context/core/src/imap.rs`](../../../context/core/src/imap.rs) — metadata fetch
- [`context/madmail/internal/endpoint/imap/imap.go`](../../../context/madmail/internal/endpoint/imap/imap.go)
- [`context/cmdeploy`](../../../context/cmdeploy/src/cmdeploy/deployers.py) — iroh-relay **v0.35.0**
- [`context/iroh`](../../../context/iroh) — tag **v0.35.0**

## Verification

```bash
cargo test -p chatmail-imap p6_s07
cargo test -p chatmail iroh_boot
```

## Linked tests

| Test ID | Step |
|---------|------|
| P6-S07 | `p6_s07_test_iroh_metadata_response`, `iroh_boot` admin toggle tests |

## Next

_See README._
