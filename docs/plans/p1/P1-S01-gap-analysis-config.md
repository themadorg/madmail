# P1-S01: Config keys, eligibility, hybrid scope

## Action

In **Delta Chat Core** (`context/core`):

1. Add `Config::WebimapTransportEnabled` → storage key `webimap_transport_enabled` (bool, default **off**).
2. Add optional `Config::WebimapBaseUrl` → `webimap_base_url` (string, empty = auto).
3. Document in `deltachat.h` / JSON-RPC schema (same as `webxdc_realtime_enabled`).
4. Implement `Context::webimap_transport_eligible()`:
   - `get_config_bool(WebimapTransportEnabled)`
   - `is_chatmail().await?`
   - configured addr + password present
   - reject when MVBOX / non-INBOX-only folder layout required
5. Write **gap matrix** comment in `src/webtransport/mod.rs` (from [10-webimap.md](../../TDD/10-webimap.md)).

## Files touched (core)

- `src/config.rs` — enum + getters
- `src/context.rs` — `get_info` / debug keys
- `deltachat-ffi/deltachat.h`
- `src/webtransport/mod.rs` — new module stub
- `src/webtransport/eligibility.rs` — pure eligibility helpers (testable)

## Tests (implement with this step)

| Test ID | Tier | Module / file | Asserts |
|---------|------|---------------|---------|
| **P1-UT00** | Unit | `src/webtransport/config_tests.rs` | `webimap_transport_enabled` unset → false; set `"1"`/`"0"` round-trip via `set_config` / `get_config` on temp context |
| **P1-UT00b** | Unit | `src/webtransport/eligibility_tests.rs` | `eligible()` false when not chatmail; false when `mvbox_move` + chats folder configured; true for minimal chatmail INBOX-only fixture |

### P1-UT00 — config round-trip

```rust
#[tokio::test]
async fn p1_ut00_webimap_transport_config_default_off() { /* ... */ }

#[tokio::test]
async fn p1_ut00_webimap_transport_config_set_get() { /* ... */ }
```

### P1-UT00b — eligibility

```rust
#[test]
fn p1_ut00b_eligible_requires_chatmail() { /* mock is_chatmail */ }

#[test]
fn p1_ut00b_eligible_rejects_mvbox_account() { /* configured folder != INBOX-only */ }
```

## Verification

```bash
cd context/core
cargo test p1_ut00
cargo test p1_ut00b
```

**Step done when:** both tests pass; `get_info` includes `webimap_transport_enabled` when set.

## Linked tests

| Test ID | Step |
|---------|------|
| P1-UT00 | P1-S01 |
| P1-UT00b | P1-S01 |

## Next

[P1-S02-core-protocol-types.md](P1-S02-core-protocol-types.md)
