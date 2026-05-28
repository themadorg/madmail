# P9-S04: IMAP GETMETADATA TURN (Chatmail key)

## Action

Replace METADATA stub (`/private/turn/relay`, `turn://`) with Madmail behaviour:

1. Advertise `METADATA` in CAPABILITY when `turn_enable` and `turn_secret` set.
2. On authenticated `GETMETADATA "" /shared/vendor/deltachat/turn` (and optionally `turns`):
   - Return RFC 5464-style response: `* METADATA "" ("/shared/vendor/deltachat/turn" "host:port:user:pass")`
3. Empty mailbox argument only; wrong mailbox → no entry or NIL per RFC 5464.
4. Respect DB/admin `turn_enabled` toggle if present.

Use `chatmail-turn::turn_metadata_line`.

## Files touched

- `crates/chatmail-imap/src/metadata.rs` (new or extend session)
- `crates/chatmail-imap/src/session.rs` — CAPABILITY
- `tests/imap_e2e.rs` — fix `imap_e2e_getmetadata_turn_relay` → `turn_imap_e2e_getmetadata_deltachat`

## TDD references

- [03-imap-server.md](../../TDD/03-imap-server.md) — Madmail-specific METADATA table
- [11-proxy-services.md](../../TDD/11-proxy-services.md)

## Madmail / context references

- `context/madmail/internal/endpoint/imap/imap.go` — `getMetadataHandler`

## RFC references

- [RFC 5464](../../TDD/RFC/rfc5464.txt) — `GETMETADATA`, shared metadata namespace
- [draft-uberti-behave-turn-rest-00](../../TDD/RFC/draft-uberti-behave-turn-rest-00.txt) — credential fields

## Verification

**P9-UT04** (alias **P6-UT02** until renamed)

```bash
cargo test -p chatmail-imap p9_ut04
cargo test -p chatmail-integration turn_imap_e2e_getmetadata_deltachat
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P9-UT04** | `P9-S04` |
| **P9-E2E01** (partial) | `P9-S04` |

## Next

[P9-S05-embed-turn-rs.md](P9-S05-embed-turn-rs.md)
