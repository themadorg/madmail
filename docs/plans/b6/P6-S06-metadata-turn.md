# P6-S06: GETMETADATA TURN

## Action

**Superseded by Phase 9** — implement real Chatmail TURN metadata in [P9-S04](../b9/P9-S04-imap-getmetadata-turn.md).

Until P9 ships: stub remains (`/private/turn/relay`). Do not treat P6 as done for production calls.

## Files touched

_See [P9-S04](../b9/P9-S04-imap-getmetadata-turn.md)._

## TDD references

- [03-imap-server.md](../../TDD/03-imap-server.md)
- [11-proxy-services.md](../../TDD/11-proxy-services.md)
- [Phase 9 plan](../b9/README.md)

## Madmail / context references

- `context/madmail/internal/endpoint/imap/imap.go`

## RFC references

- [RFC 5464](../../TDD/RFC/rfc5464.txt) — IMAP METADATA
- [draft-uberti-behave-turn-rest-00 §2.2](../../TDD/RFC/draft-uberti-behave-turn-rest-00.txt) — TURN REST credentials

## Verification

**P6-UT02** → renumbered **P9-UT04** when implemented:

```bash
cargo test -p chatmail-imap p9_ut04
cargo test -p chatmail-integration turn_imap_e2e
```

## Linked tests

| Test ID | Step |
|---------|------|
| **P6-UT02** / **P9-UT04** | P9-S04 |
| **P9-E2E01** | P9-S08, P9-S09 |

## Next

[P6-S07-metadata-iroh.md](P6-S07-metadata-iroh.md)
