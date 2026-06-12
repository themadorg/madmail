# Phase 9 — TURN/STUN for Delta Chat calls

## Goal

Ship a **production TURN relay** integrated with madmail-v2 so Delta Chat clients discover time-limited credentials over **IMAP METADATA** ([RFC 5464](https://datatracker.ietf.org/doc/html/rfc5464)) and complete WebRTC calls using **turn-rs** (or equivalent) with **TURN REST**-style HMAC secrets ([draft-uberti-behave-turn-rest-00](https://datatracker.ietf.org/doc/html/draft-uberti-behave-turn-rest-00)).

## TDD / RFC index

- [11-proxy-services.md](../../TDD/11-proxy-services.md) — architecture, contracts, test pyramid
- [03-imap-server.md](../../TDD/03-imap-server.md) — `GETMETADATA` behaviour
- [13-configuration.md](../../TDD/13-configuration.md) — `turn_*` + `turn { }` blocks
- [16-testing.md](../../TDD/16-testing.md) — E2E philosophy
- [RFC library](../../TDD/RFC/README.md) — offline specs

## Prerequisites

- Phase 5–6 IMAP: LOGIN, CAPABILITY, baseline METADATA stub ([P6-S06](../b6/P6-S06-metadata-turn.md))
- [`tests/support/`](../../tests/support/) — relay-ping-style SMTP/IMAP clients ([`imap_client.rs`](../../tests/support/imap_client.rs), [`mod.rs`](../../tests/support/mod.rs))

## Test matrix (must all pass before phase done)

| ID | Tier | Step | Command |
|----|------|------|---------|
| **P9-UT01** | Unit | P9-S01 | `cargo test -p chatmail-turn p9_ut01` |
| **P9-UT02** | Unit | P9-S02 | `cargo test -p chatmail-turn p9_ut02` |
| **P9-UT03** | Unit | P9-S03 | `cargo test -p chatmail-config turn` |
| **P9-UT04** | Unit | P9-S04 | `cargo test -p chatmail-imap p9_ut04` |
| **P9-SM01** | Smoke | P9-S05 | `cargo test -p chatmail-turn turn_smoke_stun` |
| **P9-SM02** | Smoke | P9-S06 | `cargo test -p chatmail-turn turn_smoke_allocate` |
| **P9-IT01** | Integration | P9-S07 | `cargo test -p chatmail-integration turn_metadata_auth` |
| **P9-E2E01** | E2E | P9-S08 | `cargo test -p chatmail-integration turn_imap_e2e` |
| **P9-E2E02** | E2E | P9-S09 | `make test-turn-relay-ping` (optional, running server) |
| **P9-E2E03** | E2E | P9-S10 | `scripts/core-e2e-turn.sh` (Delta Chat core) |

## Steps

| Step | File | Summary |
|------|------|---------|
| P9-S01 | [P9-S01-turn-crate-credentials.md](P9-S01-turn-crate-credentials.md) | `chatmail-turn` + HMAC metadata line |
| P9-S02 | [P9-S02-parser-core-parity.md](P9-S02-parser-core-parity.md) | Parser parity with core `calls.rs` |
| P9-S03 | [P9-S03-config-turn-blocks.md](P9-S03-config-turn-blocks.md) | `maddy.conf` `turn_*` + `turn { }` |
| P9-S04 | [P9-S04-imap-getmetadata-turn.md](P9-S04-imap-getmetadata-turn.md) | `GETMETADATA /shared/vendor/deltachat/turn` |
| P9-S05 | [P9-S05-embed-turn-rs.md](P9-S05-embed-turn-rs.md) | Start turn-rs in-process / subprocess |
| P9-S06 | [P9-S06-smoke-stun-binding.md](P9-S06-smoke-stun-binding.md) | STUN Binding smoke ([RFC 8489](https://datatracker.ietf.org/doc/html/rfc8489)) |
| P9-S07 | [P9-S07-smoke-turn-allocate.md](P9-S07-smoke-turn-allocate.md) | TURN Allocate with issued creds ([RFC 8656](https://datatracker.ietf.org/doc/html/rfc8656)) |
| P9-S08 | [P9-S08-integration-imap-turn.md](P9-S08-integration-imap-turn.md) | IMAP metadata → TURN auth integration test |
| P9-S09 | [P9-S09-e2e-relay-ping-imap.md](P9-S09-e2e-relay-ping-imap.md) | E2E raw IMAP + optional relay-ping binary |
| P9-S10 | [P9-S10-e2e-core-ice-servers.md](P9-S10-e2e-core-ice-servers.md) | Core `ice_servers()` against chatmail |

## Dependency on Phase 6

[P6-S06](../b6/P6-S06-metadata-turn.md) is superseded by **P9-S04** for the real Chatmail key and format. Keep `P6-UT02` name as alias for `P9-UT04` until renamed in code.

## Overview doc

[phase-9-implementation-plan.md](phase-9-implementation-plan.md)
